ALTER TABLE users
    ADD COLUMN mood TEXT
        CHECK (mood IS NULL OR mood IN (
            'Bored',
            'Romantic',
            'Angry',
            'Sad',
            'Happy',
            'Calm',
            'Excited',
            'Playful',
            'Lonely',
            'Grateful',
            'Stressed'
        ));
