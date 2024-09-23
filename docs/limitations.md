# Limitations of the MongoDB Data Connector

- Filtering and sorting by scalar values in arrays is not yet possible. APIPG-294
- Fields with names that begin with a dollar sign ($) or that contain dots (.) currently cannot be selected. NDC-432
- Referencing relations in mutation requests does not work. NDC-157
