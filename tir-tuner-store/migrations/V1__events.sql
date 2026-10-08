CREATE TABLE cgm (ts TIMESTAMP PRIMARY KEY, mg_dl DOUBLE NOT NULL);
CREATE TABLE bg (ts TIMESTAMP PRIMARY KEY, mg_dl DOUBLE NOT NULL);
-- Whole-row keys: the pump logs several boluses or basal segments in the same minute.
CREATE TABLE bolus (
    ts TIMESTAMP NOT NULL,
    carbs_g DOUBLE NOT NULL,
    units DOUBLE NOT NULL,
    PRIMARY KEY (ts, carbs_g, units)
);
CREATE TABLE basal (
    ts TIMESTAMP NOT NULL,
    minutes INTEGER NOT NULL,
    rate_u_h DOUBLE NOT NULL,
    PRIMARY KEY (ts, minutes, rate_u_h)
);
