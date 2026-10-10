CREATE TABLE alarm (ts TIMESTAMP NOT NULL, kind VARCHAR NOT NULL, PRIMARY KEY (ts, kind));
-- One row per settings entry, valid until the next effective_from.
CREATE TABLE profile (
    effective_from TIMESTAMP PRIMARY KEY,
    insulin_action_hours DOUBLE NOT NULL,
    max_bolus_u DOUBLE NOT NULL,
    min_glucose_mg_dl DOUBLE NOT NULL,
    weight_kg DOUBLE NOT NULL,
    total_daily_dose_u DOUBLE NOT NULL
);
-- setting: carb_ratio, correction_factor, bolus_target, auto_target.
-- end_min < start_min for a block across midnight.
CREATE TABLE profile_block (
    effective_from TIMESTAMP NOT NULL,
    setting VARCHAR NOT NULL,
    start_min SMALLINT NOT NULL,
    end_min SMALLINT NOT NULL,
    value DOUBLE NOT NULL,
    PRIMARY KEY (effective_from, setting, start_min)
);
