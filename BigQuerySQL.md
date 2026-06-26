-- This data can be extracted from https://cloud.google.com/bigquery
-- Bitcoin Transaction Forensic Data Extraction
-- Source: Google BigQuery public dataset `bigquery-public-data.crypto_bitcoin.transactions`
-- This query retrieves transaction IDs, timestamps, input (sender) addresses,
-- and output (receiver) addresses with their values in satoshis.
-- The selected date range (2019-08-11 to 2019-08-25) corresponds to a period of
-- suspected large-scale money laundering (e.g., PlusToken scam activity).
-- The results are structured to be consumed by the Rust forensic engine via CSV.

-- You can also access the downloaded data from this link:
-- [text](https://drive.google.com/drive/folders/1QuMmJGBNrnYMwr0vpwfeSh_uZv7amGZ9?usp=sharing)


SELECT
  `hash` AS tx_id,
  UNIX_SECONDS(block_timestamp) AS timestamp,

  -- Extract sender addresses from the inputs array.
  -- Only the first address of each input is taken (SAFE_OFFSET(0));
  -- inputs with empty address arrays are filtered out.
  -- The entire array of extracted addresses is serialized as a JSON string.

  TO_JSON_STRING(
    ARRAY(
      SELECT i.addresses[SAFE_OFFSET(0)]
      FROM UNNEST(inputs) AS i
      WHERE ARRAY_LENGTH(i.addresses) > 0
    )
  ) AS vin,

  -- Extract receiver addresses and their corresponding values.
  -- For each output, the first address and the value (cast to INT64) are selected.
  -- Outputs with empty address arrays are excluded.
  -- The resulting array of structs is serialized as a JSON string.

  TO_JSON_STRING(
    ARRAY(
      SELECT AS STRUCT
        o.addresses[SAFE_OFFSET(0)] AS address,
        CAST(o.value AS INT64) AS value_satoshi
      FROM UNNEST(outputs) AS o
      WHERE ARRAY_LENGTH(o.addresses) > 0
    )
  ) AS vout

FROM
  `bigquery-public-data.crypto_bitcoin.transactions`
WHERE
  -- Time window: two weeks of intensive laundering activity (adjustable as needed)
  DATE(block_timestamp) BETWEEN '2019-08-11' AND '2019-08-25'
  AND is_coinbase = FALSE      -- Exclude coinbase transactions (mining rewards)
  AND input_count > 0          -- Ensure the transaction has at least one input
  AND output_count > 0         -- Ensure the transaction has at least one output