use indexmap::IndexMap;
use std::hash::Hash;

pub fn align_with_result<K, T, U, V, E, FT, FU, FTU>(ts: IndexMap<K, T>, mut us: IndexMap<K, U>, ft: FT, fu: FU, ftu: FTU) -> Result<IndexMap<K, V>, E>
where
    K: Hash + Eq,
    FT: Fn(T) -> Result<V, E>,
    FU: Fn(U) -> Result<V, E>,
    FTU: Fn(T, U) -> Result<V, E>,
{
    let mut result: IndexMap<K, V> = IndexMap::new();

    for (k, t) in ts {
        match us.swap_remove(&k) {
            None => result.insert(k, ft(t)?),
            Some(u) => result.insert(k, ftu(t, u)?),
        };
    }

    for (k, u) in us {
        result.insert(k, fu(u)?);
    }
    Ok(result)
}
