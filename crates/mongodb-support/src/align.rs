use indexmap::IndexMap;
use std::hash::Hash;

pub fn align<K, T, U, V, FT, FU, FTU>(
    ts: IndexMap<K, T>,
    mut us: IndexMap<K, U>,
    mut ft: FT,
    mut fu: FU,
    mut ftu: FTU,
) -> IndexMap<K, V>
where
    K: Hash + Eq,
    FT: FnMut(T) -> V,
    FU: FnMut(U) -> V,
    FTU: FnMut(T, U) -> V,
{
    let mut result: IndexMap<K, V> = IndexMap::new();

    for (k, t) in ts {
        match us.swap_remove(&k) {
            None => result.insert(k, ft(t)),
            Some(u) => result.insert(k, ftu(t, u)),
        };
    }

    for (k, u) in us {
        result.insert(k, fu(u));
    }
    result
}

pub fn try_align<K, T, U, V, E, FT, FU, FTU>(
    ts: IndexMap<K, T>,
    mut us: IndexMap<K, U>,
    mut ft: FT,
    mut fu: FU,
    mut ftu: FTU,
) -> Result<IndexMap<K, V>, E>
where
    K: Hash + Eq,
    FT: FnMut(T) -> Result<V, E>,
    FU: FnMut(U) -> Result<V, E>,
    FTU: FnMut(T, U) -> Result<V, E>,
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
