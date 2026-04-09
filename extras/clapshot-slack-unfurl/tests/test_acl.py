"""Tests for channel allowlist filtering."""

from clapshot_slack_unfurl.acl import is_channel_allowed


def test_exact_channel_id_match():
    allowed = ["C04ABCD1234"]
    assert is_channel_allowed("C04ABCD1234", "random", allowed) is True
    assert is_channel_allowed("C99999999", "random", allowed) is False


def test_channel_name_regex():
    allowed = ["name:^project-x-.*"]
    assert is_channel_allowed("C111", "project-x-general", allowed) is True
    assert is_channel_allowed("C222", "project-x-design", allowed) is True
    assert is_channel_allowed("C333", "general", allowed) is False


def test_mixed_rules():
    allowed = ["C04ABCD1234", "name:^vfx-"]
    assert is_channel_allowed("C04ABCD1234", "anything", allowed) is True
    assert is_channel_allowed("C555", "vfx-review", allowed) is True
    assert is_channel_allowed("C666", "general", allowed) is False


def test_empty_allowlist_denies_all():
    assert is_channel_allowed("C111", "general", []) is False
