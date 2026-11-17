-- v1 reports were stored before reports had their own machine columns; give them the install's.
UPDATE reports r
SET windows_build = i.windows_build, ram_total = i.ram_total, ram_used_avg = i.ram_used_avg, cores = i.cores
FROM installs i
WHERE i.install_id = r.install_id AND r.ram_total IS NULL;
