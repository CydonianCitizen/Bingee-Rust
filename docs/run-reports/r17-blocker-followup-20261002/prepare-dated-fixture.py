"""Prepare an isolated copy for native Calendar and Updates navigation."""
import hashlib
import json
from pathlib import Path
import shutil
import sqlite3
import time

root = Path(__file__).resolve().parents[3]
source = root / "target/r17-validation-20261001/startup/populated-off-1/data/bingee.db"
destination = root / "target/r17-blocker-followup-20261002/final-off-heavy/data/bingee.db"
destination.parent.mkdir(parents=True, exist_ok=True)
assert not destination.exists(), "Do not overwrite a validation profile"
shutil.copyfile(source, destination)
with sqlite3.connect(destination) as connection:
    connection.create_function("bingee_fold", 1, lambda text: text.casefold(), deterministic=True)
    episode = connection.execute("SELECT e.local_media_id, e.season_number, e.episode_number FROM episodes e JOIN library_entries l ON l.local_media_id=e.local_media_id ORDER BY e.local_media_id,e.season_number,e.episode_number LIMIT 1").fetchone()
    assert episode
    connection.execute("UPDATE episodes SET air_date='2026-10-02' WHERE local_media_id=? AND season_number=? AND episode_number=?", episode)
    connection.execute("INSERT INTO release_events(local_media_id,media_type,season_number,episode_number,event_type,discovered_at,air_date) VALUES (?,'tv',?,?,'new_episode',?,'2026-10-02')", (*episode, int(time.time())))
    assert connection.execute("PRAGMA quick_check").fetchone()[0] == "ok"
record = {"source": str(source), "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(), "destination": str(destination), "destination_sha256": hashlib.sha256(destination.read_bytes()).hexdigest(), "episode": episode, "changes": "One copied episode date set to 2026-10-02; one unread release event inserted. Source fixture unchanged."}
(Path(__file__).parent / "dated-fixture.json").write_text(json.dumps(record, indent=2))
print(json.dumps(record))
