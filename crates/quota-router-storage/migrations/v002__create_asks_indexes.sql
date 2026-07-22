-- Migration v002: Create indexes on `asks` table.
--
-- idx_asker: supports "all asks by user X" queries
-- idx_model: supports cheapest-matching-Ask lookup for marketplace (Step 5 of 11-step exercise)
-- idx_expires: supports expiry filtering (used in cheapest query)

CREATE INDEX IF NOT EXISTS idx_asker       ON asks(asker_did);
CREATE INDEX IF NOT EXISTS idx_model       ON asks(model);
CREATE INDEX IF NOT EXISTS idx_expires     ON asks(expires_at_unix);