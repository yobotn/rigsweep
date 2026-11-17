-- 0.8.2: the CPU an ended process tree was using (% of the whole CPU), next to the memory it freed.
ALTER TABLE events ADD COLUMN cpu REAL NOT NULL DEFAULT 0;
