db.createView("_claims", "claims", [
  {
    $lookup: {
      from: "account_groups",
      localField: "account_group_id",
      foreignField: "account_group_id",
      as: "account_group_docs",
    },
  },
  {
    $unwind: "$account_group_docs",
  },
  {
    $lookup: {
      from: "carriers",
      localField: "account_group_docs.carrier_id",
      foreignField: "carrier_id",
      as: "carrier_docs",
    },
  },
  {
    $unwind: "$carrier_docs",
  },
  {
    $lookup: {
      from: "companies",
      localField: "carrier_docs.company_id",
      foreignField: "id",
      as: "company_docs",
    },
  },
  {
    $unwind: "$company_docs",
  },
  {
    $project: {
      _id: "$id",
      prescription_description: 1,
      amount: 1,
      date: 1,
      status: 1,
      patient_id: 1,
      account_group_name: "$account_group_docs.account_group_name",
      carrier_name: "$carrier_docs.carrier_name",
      company_name: "$company_docs.company_name",
      company_location: "$company_docs.company_location",
    },
  },
]);
