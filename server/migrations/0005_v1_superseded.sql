-- An install's first v2 report carries its whole local history row by row, which repeats the totals
-- its v1 (weekly) reports already sent. Where both exist, the v1 rows (no seq) go; the v2 rows stay.
DELETE FROM events e
WHERE e.seq IS NULL
  AND EXISTS (SELECT 1 FROM events v2 WHERE v2.install_id = e.install_id AND v2.seq IS NOT NULL);
UPDATE reports r SET processes = NULL, cleanups = NULL
WHERE r.v = 1 AND EXISTS (SELECT 1 FROM events v2 WHERE v2.install_id = r.install_id AND v2.seq IS NOT NULL);
