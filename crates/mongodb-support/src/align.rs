use indexmap::IndexMap;
use std::hash::Hash;

pub fn align<K, T, U, V, FT, FU, FTU>(ts: IndexMap<K, T>, mut us: IndexMap<K, U>, ft: FT, fu: FU, ftu: FTU) -> IndexMap<K, V>
where
    K: Hash + Eq,
    FT: Fn(T) -> V,
    FU: Fn(U) -> V,
    FTU: Fn(T, U) -> V,
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
