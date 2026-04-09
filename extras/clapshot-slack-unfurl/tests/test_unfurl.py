"""Tests for the unfurl pipeline: URL parsing → DB → Block Kit content."""

from clapshot_slack_unfurl.unfurl import parse_clapshot_url, build_unfurl, ParsedURL
from clapshot_slack_unfurl.db import open_db, get_media_file, get_comment


def test_parse_video_url():
    r = parse_clapshot_url("https://clapshot.example.local/?vid=a1b2c3d4")
    assert r == ParsedURL(video_id="a1b2c3d4", comment_id=None)


def test_parse_comment_url():
    r = parse_clapshot_url("https://clapshot.example.local/?vid=a1b2c3d4#comment_42")
    assert r == ParsedURL(video_id="a1b2c3d4", comment_id="42")


def test_parse_non_clapshot_url():
    assert parse_clapshot_url("https://example.com/foo") is None


def test_db_get_media_file(clapshot_env):
    db = open_db(clapshot_env["db_path"])
    row = get_media_file(db, "a1b2c3d4")
    assert row is not None
    assert row.title == "Hero Shot v3"
    assert row.owner == "Alice Anderson"
    assert row.duration == 125.7


def test_db_get_media_file_no_title(clapshot_env):
    db = open_db(clapshot_env["db_path"])
    row = get_media_file(db, "deadbeef")
    assert row is not None
    assert row.title == "untitled.mov"  # falls back to orig_filename


def test_db_get_comment(clapshot_env):
    db = open_db(clapshot_env["db_path"])
    row = get_comment(db, "42")
    assert row is not None
    assert row.comment == "Fix the lighting here"
    assert row.drawing == "abcdef12.webp"
    assert row.commenter == "Bob Builder"


def test_build_unfurl_for_video(clapshot_env):
    result = build_unfurl(
        url="https://clapshot.example.local/?vid=a1b2c3d4",
        db_path=clapshot_env["db_path"],
        videos_dir=clapshot_env["videos_dir"],
    )
    assert result is not None
    assert result.title == "Hero Shot v3"
    assert ("Duration", "2:05") in result.fields
    assert ("Owner", "Alice Anderson") in result.fields
    assert result.text is None
    assert result.image_bytes is not None
    assert result.image_filename.endswith(".jpg")


def test_build_unfurl_for_missing_video(clapshot_env):
    result = build_unfurl(
        url="https://clapshot.example.local/?vid=00000000",
        db_path=clapshot_env["db_path"],
        videos_dir=clapshot_env["videos_dir"],
    )
    assert result is None
