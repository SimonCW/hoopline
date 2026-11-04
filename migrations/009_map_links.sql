ALTER TABLE slots ADD COLUMN map_url TEXT NOT NULL DEFAULT '';
ALTER TABLE slot_schedules ADD COLUMN map_url TEXT NOT NULL DEFAULT '';

UPDATE slots SET map_url = CASE venue
    WHEN 'Luisenschule' THEN 'https://www.google.com/maps/search/?api=1&query=Luisen+Gymnasium%2C+Walther-Rathenau-Stra%C3%9Fe+99%2C+33602+Bielefeld'
    WHEN 'Luisen' THEN 'https://www.google.com/maps/search/?api=1&query=Luisen+Gymnasium%2C+Walther-Rathenau-Stra%C3%9Fe+99%2C+33602+Bielefeld'
    WHEN 'Ceci' THEN 'https://www.google.com/maps/search/?api=1&query=Ceciliengymnasium+Halle+1%2C+Niederm%C3%BChlenkamp+5%2C+33604+Bielefeld'
    WHEN 'Diesterweg' THEN 'https://www.google.com/maps/search/?api=1&query=Diesterwegschule%2C+Rohrteichstra%C3%9Fe+73%2C+33602+Bielefeld'
    ELSE '' END;

UPDATE slot_schedules SET map_url = CASE venue
    WHEN 'Luisenschule' THEN 'https://www.google.com/maps/search/?api=1&query=Luisen+Gymnasium%2C+Walther-Rathenau-Stra%C3%9Fe+99%2C+33602+Bielefeld'
    WHEN 'Luisen' THEN 'https://www.google.com/maps/search/?api=1&query=Luisen+Gymnasium%2C+Walther-Rathenau-Stra%C3%9Fe+99%2C+33602+Bielefeld'
    WHEN 'Ceci' THEN 'https://www.google.com/maps/search/?api=1&query=Ceciliengymnasium+Halle+1%2C+Niederm%C3%BChlenkamp+5%2C+33604+Bielefeld'
    WHEN 'Diesterweg' THEN 'https://www.google.com/maps/search/?api=1&query=Diesterwegschule%2C+Rohrteichstra%C3%9Fe+73%2C+33602+Bielefeld'
    ELSE '' END;
