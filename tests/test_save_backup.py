import datetime
import tempfile
import unittest
from pathlib import Path

import paths  # noqa: F401  (puts scripts/ on sys.path)
import save_backup


def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding='utf-8')


class SaveBackupTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.user = Path(self.temp.name)
        self.save = self.user / 'savedata' / '1' / 'CUSA01623' / 'AUTOSAVE' / 'data.bin'
        self.clock = datetime.datetime(2026, 10, 9, 18, 0, 0)

    def tearDown(self):
        self.temp.cleanup()

    def tick(self):
        self.clock += datetime.timedelta(minutes=1)
        return self.clock

    def test_copies_savedata(self):
        write(self.save, 'v1')
        made = save_backup.backup(self.user, now=self.tick())
        self.assertEqual(made.name, '20261009-180100')
        self.assertEqual((made / self.save.relative_to(self.user / 'savedata')).read_text(), 'v1')

    def test_no_backup_without_saves(self):
        self.assertIsNone(save_backup.backup(self.user, now=self.tick()))
        (self.user / 'savedata').mkdir()
        self.assertIsNone(save_backup.backup(self.user, now=self.tick()))
        self.assertEqual(save_backup.list_backups(self.user), [])

    def test_same_second_gets_unique_name(self):
        write(self.save, 'v1')
        first = save_backup.backup(self.user, now=self.clock)
        second = save_backup.backup(self.user, now=self.clock)
        self.assertNotEqual(first, second)
        self.assertEqual(len(save_backup.list_backups(self.user)), 2)

    def test_keeps_newest_ten(self):
        write(self.save, 'v')
        made = [save_backup.backup(self.user, now=self.tick()) for _ in range(12)]
        kept = save_backup.list_backups(self.user)
        self.assertEqual(len(kept), save_backup.KEEP)
        self.assertEqual(kept[0], made[-1].name)  # newest first
        self.assertFalse(made[0].exists())
        self.assertFalse(made[1].exists())

    def test_restore_backs_up_current_first(self):
        write(self.save, 'old')
        old = save_backup.backup(self.user, now=self.tick())
        write(self.save, 'new')
        write(self.save.parent / 'extra.bin', 'x')
        safety = save_backup.restore(self.user, old.name, now=self.tick())
        self.assertEqual(self.save.read_text(), 'old')
        self.assertFalse((self.save.parent / 'extra.bin').exists())
        self.assertEqual((safety / self.save.relative_to(self.user / 'savedata')).read_text(), 'new')

    def test_restore_oldest_when_full(self):
        write(self.save, 'oldest')
        oldest = save_backup.backup(self.user, now=self.tick())
        write(self.save, 'later')
        for _ in range(save_backup.KEEP - 1):
            save_backup.backup(self.user, now=self.tick())
        save_backup.restore(self.user, oldest.name, now=self.tick())
        self.assertEqual(self.save.read_text(), 'oldest')
        self.assertEqual(len(save_backup.list_backups(self.user)), save_backup.KEEP)

    def test_restore_rejects_unknown_or_unsafe_names(self):
        write(self.save, 'v')
        for name in ('missing', '..', '../savedata', ''):
            with self.assertRaises(ValueError):
                save_backup.restore(self.user, name, now=self.tick())
        self.assertEqual(self.save.read_text(), 'v')


if __name__ == '__main__':
    unittest.main()
