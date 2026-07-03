"""Tests for comment unfurl with drawing fallback."""

import sqlite3
from PIL import Image
from clapshot_slack_unfurl.unfurl import build_unfurl


def test_comment_with_drawing_uses_drawing(clapshot_env):
    """Comment 42 has a drawing — unfurl should use the drawing image."""
    result = build_unfurl(
        url="https://clapshot.example.local/?vid=a1b2c3d4#comment_42",
        db_path=clapshot_env["db_path"],
        videos_dir=clapshot_env["videos_dir"],
    )
    assert result is not None
    assert "drawing" in result.image_filename
    assert result.text == "Fix the lighting here"
    assert ("By", "Bob Builder") in result.fields
    assert result.title == "Comment on: Hero Shot v3"


def test_comment_without_drawing_falls_back_to_thumbnail(clapshot_env):
    """Comment 43 has no drawing — unfurl should use the video thumbnail."""
    result = build_unfurl(
        url="https://clapshot.example.local/?vid=a1b2c3d4#comment_43",
        db_path=clapshot_env["db_path"],
        videos_dir=clapshot_env["videos_dir"],
    )
    assert result is not None
    assert "drawing" not in result.image_filename
    assert "thumb" in result.image_filename
    assert result.text == "Looks good!"
    assert ("By", "Alice Anderson") in result.fields


def test_nonexistent_comment_falls_back_to_video(clapshot_env):
    """Comment ID doesn't exist — should fall back to plain video unfurl."""
    result = build_unfurl(
        url="https://clapshot.example.local/?vid=a1b2c3d4#comment_999",
        db_path=clapshot_env["db_path"],
        videos_dir=clapshot_env["videos_dir"],
    )
    assert result is not None
    assert result.title == "Hero Shot v3"  # plain video unfurl
    assert "thumb" in result.image_filename


def test_path_traversal_in_drawing_filename_is_blocked(clapshot_env, tmp_path):
    """A malicious drawing filename must not escape videos_dir."""
    videos_dir = clapshot_env["videos_dir"]

    # Place a secret image OUTSIDE videos_dir that traversal would reach
    secret = tmp_path / "secret.webp"
    Image.new("RGB", (10, 10), color=(255, 0, 0)).save(secret)

    # Compute the relative traversal from videos_dir/a1b2c3d4/drawings/ to secret
    traversal = "../../../secret.webp"

    # Insert a comment with the malicious drawing path
    conn = sqlite3.connect(clapshot_env["db_path"])
    conn.execute(
        "UPDATE comments SET drawing = ? WHERE id = 99", (traversal,))
    conn.commit()
    conn.close()

    result = build_unfurl(
        url="https://clapshot.example.local/?vid=a1b2c3d4#comment_99",
        db_path=clapshot_env["db_path"],
        videos_dir=videos_dir,
    )
    assert result is not None
    # Must NOT have read the secret file — should fall back to video thumbnail
    assert "drawing" not in result.image_filename
    assert "thumb" in result.image_filename
