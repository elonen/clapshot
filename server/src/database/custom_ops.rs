use anyhow::Context;
use diesel::prelude::*;
use chrono::offset::Local;
use uuid::Uuid;
use crate::{database::{models, schema, to_db_res, DBResult, EmptyDBResult}, retry_if_db_locked};

use super::{error::DBError, DbBasicQuery, PooledConnection};

// ------------------- Model-specific custom operations -------------------

impl models::User {
    pub fn set_name(conn: &mut PooledConnection, uid: &str, new_name: &str) -> EmptyDBResult
    {
        use schema::users::dsl::*;
        retry_if_db_locked!({
            diesel::update(users.filter(id.eq(uid)))
                .set(name.eq(new_name))
                .execute(conn)
        })?;
        Ok(())
    }

    /// Get a user by ID, or create a new user if it doesn't exist.
    ///
    /// # Arguments
    /// * `conn` - Database connection
    /// * `user_id` - ID of the user
    /// * `username` - Name of the user, if you want to update it. If None, and user is being created, the name will be set to the user_id.
    pub fn get_or_create(conn: &mut PooledConnection, user_id: &str, username: Option<&str>) -> DBResult<models::User>
    {
        match models::User::get(conn, &user_id.to_string()) {
            Ok(u) => {
                // Update name if needed
                if let Some(username) = username {
                    models::User::set_name(conn, &u.id, &username).context("Failed to update user name")?;
                }
                models::User::get(conn, &u.id)
            },
            Err(DBError::NotFound()) => {
                // User not found, create a new user
                let new_user = models::UserInsert {
                    id: user_id.to_string(),
                    name: username.unwrap_or(user_id).to_string(),
                };
                models::User::insert(conn, &new_user)
            },
            Err(e) => { Err(e) }
        }
    }
}


impl models::VersionSet {

    /// Create a new version set with a fresh UUID.
    pub fn create_new(conn: &mut PooledConnection) -> DBResult<models::VersionSet> {
        let new_id = Uuid::new_v4().to_string();
        models::VersionSet::insert(conn, &models::VersionSetInsert { id: new_id })
    }

    /// Delete a version set only if it has fewer than 2 members left.
    /// Called after ungrouping a media file to clean up empty/singleton sets.
    pub fn cleanup_if_empty(conn: &mut PooledConnection, vs_id: &str) -> EmptyDBResult {
        use schema::media_files::dsl as mf;
        let count: i64 = mf::media_files
            .filter(mf::version_set_id.eq(vs_id))
            .count()
            .get_result(conn)
            .map_err(|e| anyhow::anyhow!("{:?}", e))?;
        if count < 2 {
            // Clear the remaining member(s) and delete the set
            retry_if_db_locked!({
                diesel::update(mf::media_files.filter(mf::version_set_id.eq(vs_id)))
                    .set(mf::version_set_id.eq(Option::<String>::None))
                    .execute(conn)
            })?;
            use schema::version_sets::dsl as vs;
            retry_if_db_locked!({
                diesel::delete(vs::version_sets.filter(vs::id.eq(vs_id))).execute(conn)
            })?;
        }
        Ok(())
    }
}


impl models::MediaFile {

    /// Set the recompressed flag for a media file.
    ///
    /// # Arguments
    /// * `db` - Database
    /// * `vid` - Id of the media file
    pub fn set_recompressed(conn: &mut PooledConnection, vid: &str) -> EmptyDBResult
    {
        use schema::media_files::dsl::*;
        retry_if_db_locked!({
            diesel::update(media_files.filter(id.eq(vid)))
                .set(recompression_done.eq(Local::now().naive_local()))
                .execute(conn)
        })?;
        Ok(())
    }

    /// Update fps and total frame count for a media file.
    ///
    /// At ingest these come from the source file, which has no fps for audio. After
    /// transcoding they are refreshed from the actual playable (transcoded) output,
    /// so the client can compute valid SMPTE timecodes.
    ///
    /// # Arguments
    /// * `conn` - Database connection
    /// * `vid` - Id of the media file
    /// * `new_fps` - Frames per second (as stored, e.g. "60.000")
    /// * `new_total_frames` - Total number of frames in the transcoded video
    pub fn set_fps_and_frame_count(conn: &mut PooledConnection, vid: &str, new_fps: &str, new_total_frames: i32) -> EmptyDBResult
    {
        use schema::media_files::dsl::*;
        retry_if_db_locked!({
            diesel::update(media_files.filter(id.eq(vid)))
                .set((fps.eq(new_fps), total_frames.eq(new_total_frames)))
                .execute(conn)
        })?;
        Ok(())
    }

    /// Set default subtitle id for a media file.
    ///
    /// # Arguments
    /// * `db` - Database
    /// * `vid` - Id of the media file
    /// * `sid` - Id of the subtitle
    pub fn set_default_subtitle(conn: &mut PooledConnection, vid: &str, sid: Option<i32>) -> EmptyDBResult
    {
        use schema::media_files::dsl::*;
        retry_if_db_locked!({
            diesel::update(media_files.filter(id.eq(vid)))
                .set(default_subtitle_id.eq(sid))
                .execute(conn)
        })?;
        Ok(())
    }

    /// Set thumbnail sheet dimensions for a media file.
    ///
    /// # Arguments
    /// * `db` - Database
    /// * `vid` - Id of the media file
    /// * `cols` - Width of the thumbnail sheet
    /// * `rows` - Height of the thumbnail sheet
    pub fn set_thumb_sheet_dimensions(conn: &mut PooledConnection, vid: &str, cols: u32, rows: u32) -> EmptyDBResult
    {
        use schema::media_files::dsl::*;
        retry_if_db_locked!({
            diesel::update(media_files.filter(id.eq(vid)))
                .set((thumb_sheet_cols.eq(cols as i32), thumb_sheet_rows.eq(rows as i32)))
                .execute(conn)
        })?;
        Ok(())
    }

    /// Set the thumbnail flag for a media file.
    ///
    /// # Arguments
    /// * `db` - Database
    /// * `vid` - Id of the media file
    /// * `new_value` - New value of the flag
    pub fn set_has_thumb(conn: &mut PooledConnection, vid: &str, new_value: bool) -> EmptyDBResult
    {
        use schema::media_files::dsl::*;
        retry_if_db_locked!({
            diesel::update(media_files.filter(id.eq(vid)))
                .set(has_thumbnail.eq(new_value))
                .execute(conn)
        })?;
        Ok(())
    }

    /// Set the thumbs_done timestamp for a media file.
    /// This is used to indicate that the thumbnail generation is done (wether anything was generated or not).
    ///
    /// # Arguments
    /// * `db` - Database
    /// * `vid` - Id of the media file
    pub fn set_thumbs_done(conn: &mut PooledConnection, vid: &str) -> EmptyDBResult
    {
        use schema::media_files::dsl::*;
        retry_if_db_locked!({
            diesel::update(media_files.filter(id.eq(vid)))
                .set(thumbs_done.eq(Local::now().naive_local()))
                .execute(conn)
        })?;
        Ok(())
    }


    /// Rename a media file (title).
    ///
    /// # Arguments
    /// * `db` - Database
    /// * `vid` - Id of the media file
    /// * `new_name` - New title
    ///
    /// # Returns
    /// * `EmptyResult`
    /// * `Err(NotFound)` - MediaFile not found
    /// * `Err(Other)` - Other error
    pub fn rename(conn: &mut PooledConnection, vid: &str, new_name: &str) -> EmptyDBResult
    {
        use schema::media_files::dsl::*;
        retry_if_db_locked!({
            diesel::update(media_files.filter(id.eq(vid)))
                .set(title.eq(new_name))
                .execute(conn)
        })?;
        Ok(())
    }

    /// Get all media files that don't have their thumbnails generated.
    ///
    /// # Returns
    /// * `Vec<models::MediaFile>` - List of MediaFile objects
    pub fn get_all_with_missing_thumbnails(conn: &mut PooledConnection) -> DBResult<Vec<models::MediaFile>>
    {
        use models::*;
        use schema::media_files::dsl::*;
        to_db_res(retry_if_db_locked!({ media_files.filter(thumbs_done.is_null()).order_by(added_time.desc()).load::<MediaFile>(conn) }))
    }

    /// Return all media files in the same version set as `media_file_id`, excluding itself.
    pub fn get_version_siblings(conn: &mut PooledConnection, media_file_id: &str) -> DBResult<Vec<models::MediaFile>> {
        use schema::media_files::dsl as mf;

        let vs_id: Option<String> = mf::media_files
            .filter(mf::id.eq(media_file_id))
            .select(mf::version_set_id)
            .first(conn)
            .map_err(|e| DBError::BackendError(e))?;

        match vs_id {
            None => Ok(vec![]),
            Some(vs) => {
                let siblings = mf::media_files
                    .filter(mf::version_set_id.eq(&vs))
                    .filter(mf::id.ne(media_file_id))
                    .order_by(mf::added_time.asc())
                    .load::<models::MediaFile>(conn)
                    .map_err(|e| DBError::BackendError(e))?;
                Ok(siblings)
            }
        }
    }

    /// Group `media_file_id` together with `group_with_id` into the same version set.
    ///
    /// Handles all cases:
    /// - Neither in a set → create a new set for both
    /// - One in a set → add the other to the existing set
    /// - Both in different sets → merge: move all members of the smaller set into the larger
    pub fn group_with(conn: &mut PooledConnection, media_file_id: &str, group_with_id: &str) -> EmptyDBResult {
        use schema::media_files::dsl as mf;

        if media_file_id == group_with_id {
            return Err(anyhow::anyhow!("Cannot group a video with itself").into());
        }

        let a: models::MediaFile = mf::media_files.filter(mf::id.eq(media_file_id))
            .first(conn).map_err(|_| DBError::NotFound())?;
        let b: models::MediaFile = mf::media_files.filter(mf::id.eq(group_with_id))
            .first(conn).map_err(|_| DBError::NotFound())?;

        let target_set_id = match (a.version_set_id.as_deref(), b.version_set_id.as_deref()) {
            (None, None) => {
                // Create a new version set and assign both
                let vs = models::VersionSet::create_new(conn)?;
                retry_if_db_locked!({
                    diesel::update(mf::media_files.filter(mf::id.eq(media_file_id).or(mf::id.eq(group_with_id))))
                        .set(mf::version_set_id.eq(&vs.id))
                        .execute(conn)
                })?;
                return Ok(());
            }
            (Some(set_a), None) => set_a.to_string(),
            (None, Some(set_b)) => set_b.to_string(),
            (Some(set_a), Some(set_b)) => {
                if set_a == set_b {
                    return Ok(()); // Already in the same set
                }
                // Merge set_b into set_a (move all of B's members to A's set)
                retry_if_db_locked!({
                    diesel::update(mf::media_files.filter(mf::version_set_id.eq(set_b)))
                        .set(mf::version_set_id.eq(set_a))
                        .execute(conn)
                })?;
                use schema::version_sets::dsl as vs;
                retry_if_db_locked!({
                    diesel::delete(vs::version_sets.filter(vs::id.eq(set_b))).execute(conn)
                })?;
                return Ok(());
            }
        };

        // Assign the ungrouped video to the existing target set
        let ungrouped_id = if a.version_set_id.is_none() { media_file_id } else { group_with_id };
        retry_if_db_locked!({
            diesel::update(mf::media_files.filter(mf::id.eq(ungrouped_id)))
                .set(mf::version_set_id.eq(&target_set_id))
                .execute(conn)
        })?;
        Ok(())
    }

    /// Remove `media_file_id` from its version set. If fewer than 2 members remain, the set is dissolved.
    pub fn ungroup(conn: &mut PooledConnection, media_file_id: &str) -> EmptyDBResult {
        use schema::media_files::dsl as mf;

        let vs_id: Option<String> = mf::media_files
            .filter(mf::id.eq(media_file_id))
            .select(mf::version_set_id)
            .first(conn)
            .map_err(|e| DBError::BackendError(e))?;

        let vs_id = match vs_id {
            None => return Ok(()), // Not in any set, nothing to do
            Some(id) => id,
        };

        // Remove this file from the set
        retry_if_db_locked!({
            diesel::update(mf::media_files.filter(mf::id.eq(media_file_id)))
                .set(mf::version_set_id.eq(Option::<String>::None))
                .execute(conn)
        })?;

        // Clean up the set if it now has fewer than 2 members
        models::VersionSet::cleanup_if_empty(conn, &vs_id)?;
        Ok(())
    }
}


impl models::Comment {

    /// Edit a comment (change text).
    ///
    /// # Arguments
    /// * `comment_id` - ID of the comment
    /// * `new_comment` - New text of the comment
    ///
    /// # Returns
    /// * `Res<bool>` - True if comment was edited, false if it was not found
    pub fn edit(conn: &mut PooledConnection, comment_id: i32, new_comment: &str) -> DBResult<bool>
    {
        use schema::comments::dsl::*;
        to_db_res(retry_if_db_locked!({
            diesel::update(comments.filter(id.eq(comment_id)))
                .set((comment.eq(new_comment), edited.eq(diesel::dsl::now))).execute(conn).map(|x| x > 0)
        }))
    }
}


impl models::Message {

    /// Set the seen status of a message.
    ///
    /// # Arguments
    /// * `db` - Database
    /// * `msg_id` - ID of the message
    /// * `new_status` - New status
    ///
    /// # Returns
    /// * `Res<bool>` - True if message was found and updated, false if it was not found
    pub fn set_seen(conn: &mut PooledConnection, msg_id: i32, new_status: bool) -> DBResult<bool>
    {
        use schema::messages::dsl::*;
        to_db_res(retry_if_db_locked!({
            diesel::update(messages.filter(id.eq(msg_id)))
                .set(seen.eq(new_status)).execute(conn).map(|x| x > 0)
        }))
    }

    /// Get all messages for a given comment.
    ///
    /// # Arguments
    /// * `db` - Database
    /// * `cid` - ID of the comment
    ///
    /// # Returns
    /// * `Res<Vec<models::Message>>` - List of messages
    pub fn get_by_comment(conn: &mut PooledConnection, cid: i32) -> DBResult<Vec<models::Message>>
    {
        use schema::messages::dsl::*;
        to_db_res(retry_if_db_locked!({
            messages.filter(comment_id.eq(cid)).order(created.desc()).load::<models::Message>(conn)
        }))
    }
}
