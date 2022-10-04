"""empty message

Revision ID: 74e3b28d0f00
Revises: f4dba652e0c2
Create Date: 2022-10-05 00:14:05.400483

"""
from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision = '74e3b28d0f00'
down_revision = 'f4dba652e0c2'
branch_labels = None
depends_on = None


def upgrade() -> None:

    op.drop_index('ix_comment_parent_id', table_name='comment')
    op.drop_index('ix_video_added_by_userid', table_name='video')
    op.drop_index('ix_video_video_hash', table_name='video')

    conn = op.get_bind()
    conn.execute("ALTER TABLE video RENAME TO videos;")
    conn.execute("ALTER TABLE comment RENAME TO comments;")
    conn.execute("ALTER TABLE message RENAME TO messages;")

    op.create_index(op.f('ix_comments_parent_id'), 'comments', ['parent_id'], unique=False)
    op.create_index(op.f('ix_videos_added_by_userid'), 'videos', ['added_by_userid'], unique=False)
    op.create_index(op.f('ix_videos_video_hash'), 'videos', ['video_hash'], unique=True)


def downgrade() -> None:

    op.drop_index('ix_comments_parent_id', table_name='comments')
    op.drop_index('ix_videos_added_by_userid', table_name='videos')
    op.drop_index('ix_videos_video_hash', table_name='videos')

    conn = op.get_bind()
    conn.execute("ALTER TABLE messages RENAME TO message;")
    conn.execute("ALTER TABLE comments RENAME TO comment;")
    conn.execute("ALTER TABLE videos RENAME TO video;")

    op.create_index(op.f('ix_comment_parent_id'), 'comment', ['parent_id'], unique=False)
    op.create_index(op.f('ix_video_added_by_userid'), 'video', ['added_by_userid'], unique=False)
    op.create_index(op.f('ix_video_video_hash'), 'video', ['video_hash'], unique=True)
