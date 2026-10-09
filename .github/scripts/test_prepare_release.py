import unittest
from prepare_release import next_version


class ReleaseVersions(unittest.TestCase):
    def test_patch_after_latest_tag_even_when_main_version_is_older(self):
        self.assertEqual(next_version("0.3.0", ["v0.3.0", "v0.3.2"]), "0.3.3")

    def test_explicit_minor_bump_is_preserved(self):
        self.assertEqual(next_version("0.4.0", ["v0.3.2"]), "0.4.0")

    def test_numeric_order(self):
        self.assertEqual(next_version("1.0.0", ["v1.0.9", "v1.0.10"]), "1.0.11")

    def test_unrelated_and_prerelease_tags_are_ignored(self):
        self.assertEqual(next_version("0.1.0", ["other", "v0.2.0-beta"]), "0.1.0")

    def test_unstable_package_version_is_rejected(self):
        with self.assertRaises(ValueError):
            next_version("0.1.0-beta", [])


if __name__ == "__main__":
    unittest.main()
