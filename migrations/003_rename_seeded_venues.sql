UPDATE slots
SET venue = CASE venue
    WHEN 'Court A' THEN 'Luisenschule'
    WHEN 'Court B' THEN 'Ceci'
    WHEN 'Court C' THEN 'Diesterweg'
    ELSE venue
END
WHERE venue IN ('Court A', 'Court B', 'Court C');
