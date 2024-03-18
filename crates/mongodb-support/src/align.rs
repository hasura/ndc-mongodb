use indexmap::IndexMap;
use std::hash::Hash;
use these::These::{self, *};

pub fn align<K, T, U>(ts: IndexMap<K, T>, mut us: IndexMap<K, U>) -> IndexMap<K, These<T, U>>
where
    K: Hash + Eq,
{
    let mut result: IndexMap<K, These<T, U>> = IndexMap::new();

    for (k, t) in ts {
        match us.swap_remove(&k) {
            None => result.insert(k, This(t)),
            Some(u) => result.insert(k, Both(t, u)),
        };
    }

    for (k, u) in us {
        result.insert(k, That(u));
    }
    result
}

pub fn align_with<K, V, F>(ts: IndexMap<K, V>, mut us: IndexMap<K, V>, f: F) -> IndexMap<K, V>
where
    K: Hash + Eq,
    F: Fn(V, V) -> V,
{
    let mut result: IndexMap<K, V> = IndexMap::new();

    for (k, t) in ts {
        match us.swap_remove(&k) {
            None => result.insert(k, t),
            Some(u) => result.insert(k, f(t, u)),
        };
    }

    for (k, u) in us {
        result.insert(k, u);
    }
    result
}

// pub fn align_with_result<K, V, E, F>(ts: IndexMap<K, V>, mut us: IndexMap<K, V>, f: F) -> Result<IndexMap<K, V>, E>
// where
//     K: Hash + Eq,
//     F: Fn(V, V) -> Result<V, E>,
// {
//     let mut result: IndexMap<K, V> = IndexMap::new();

//     for (k, t) in ts {
//         match us.swap_remove(&k) {
//             None => result.insert(k, t),
//             Some(u) => result.insert(k, f(t, u)?),
//         };
//     }

//     for (k, u) in us {
//         result.insert(k, u);
//     }
//     Ok(result)
// }

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
