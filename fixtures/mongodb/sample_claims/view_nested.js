db.createView(
  "_companies",
  "companies",
  [
    {
      $lookup: {
        from: "carriers",
        localField: "id",
        foreignField: "company_id",
        as: "carriers"
      }
    },
    { $unwind: { path: "$carriers", preserveNullAndEmptyArrays: true } },
    {
      $lookup: {
        from: "account_groups",
        let: { 
          carrier_ids: { 
            $cond: { 
              if: { $isArray: "$carriers.carrier_id" }, 
              then: "$carriers.carrier_id", 
              else: ["$carriers.carrier_id"]
            } 
          } 
        },
        pipeline: [
          {
            $match: {
              $expr: {
                $in: ["$carrier_id", "$$carrier_ids"]
              }
            }
          }
        ],
        as: "account_groups"
      }
    },
    { $unwind: { path: "$account_groups", preserveNullAndEmptyArrays: true } },
    {
      $lookup: {
        from: "claims",
        let: { 
          account_group_ids: { 
            $cond: { 
              if: { $isArray: "$account_groups.account_group_id" }, 
              then: "$account_groups.account_group_id", 
              else: ["$account_groups.account_group_id"]
            } 
          } 
        },
        pipeline: [
          {
            $match: {
              $expr: {
                $in: ["$account_group_id", "$$account_group_ids"]
              }
            }
          }
        ],
        as: "claims"
      }
    },
    { $unwind: { path: "$claims", preserveNullAndEmptyArrays: true } },
    {
      $group: {
        _id: "$id",
        company_name: { $first: "$company_name" },
        company_location: { $first: "$company_location" },
        carriers: { $addToSet: "$carriers" },
        account_groups: { $addToSet: "$account_groups" },
        claims: { $addToSet: "$claims" }
      }
    },
    {
      $project: {
        _id: "$_id",
        company_name: 1,
        company_location: 1,
        carriers: 1,
        account_groups: 1,
        claims: 1
      }
    }
  ]
)