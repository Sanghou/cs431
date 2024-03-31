//! Thread-safe key/value cache.

use std::collections::hash_map::{Entry, HashMap};
use std::hash::Hash;
use std::sync::{Arc, Condvar, Mutex, RwLock};

type MutexHashMap<K> = Mutex<HashMap<K, Arc<(Mutex<bool>, Condvar)>>>;

/// Cache that remembers the result for each key.
#[derive(Debug)]
pub struct Cache<K, V> {
    inner: HashMap<K, Arc<V>>,
    cond_per_key: MutexHashMap<K>,
}

impl<K, V> Default for Cache<K, V> {
    fn default() -> Self {
        Self {
            inner: HashMap::new(),
            cond_per_key: Mutex::new(HashMap::new()),
        }
    }
}

impl<K: Eq + Hash + Clone, V: Clone> Cache<K, V> {
    /// Retrieve the value or insert a new one created by `f`.
    ///
    /// An invocation to this function should not block another invocation with a different key. For
    /// example, if a thread calls `get_or_insert_with(key1, f1)` and another thread calls
    /// `get_or_insert_with(key2, f2)` (`key1≠key2`, `key1,key2∉cache`) concurrently, `f1` and `f2`
    /// should run concurrently.
    ///
    /// On the other hand, since `f` may consume a lot of resource (= money), it's undesirable to
    /// duplicate the work. That is, `f` should be run only once for each key. Specifically, even
    /// for concurrent invocations of `get_or_insert_with(key, f)`, `f` is called only once per key.
    ///
    /// Hint: the [`Entry`] API may be useful in implementing this function.
    ///
    /// [`Entry`]: https://doc.rust-lang.org/stable/std/collections/hash_map/struct.HashMap.html#method.entry
    ///
    pub fn get_or_insert_with<F: FnOnce(K) -> V>(&self, key: K, f: F) -> V {
        let mut inner_clone = self.inner.clone();
        let value = inner_clone.entry(key.clone());

        match value {
            Entry::Occupied(entry) => {
                // value 있음
                let val = entry.get().clone();
                (*val).clone()
            }
            Entry::Vacant(entry) => {
                // value 없음 => cond_mapper에서 계산중인지 확인하기
                let mut cond_mapper = self.cond_per_key.lock().unwrap();
                if cond_mapper.contains_key(&key) {
                    let cond_elem = cond_mapper.get(&key).unwrap();

                    let mut cond_bool = cond_elem.0.lock().unwrap();

                    while *cond_bool {
                        cond_bool = cond_elem.1.wait(cond_elem.0.lock().unwrap()).unwrap();
                    }
                    let calculated_val = inner_clone.get(&key).unwrap().clone();

                    calculated_val.as_ref().clone()
                } else {
                    // condVar 생성
                    let cond_elem = Arc::new((Mutex::new(true), Condvar::new()));
                    cond_mapper.insert(key.clone(), cond_elem.clone());

                    // 캐시 업데이트
                    let res = f(key.clone());
                    inner_clone.insert(key, Arc::new(res.clone()));

                    // condVar 재업데이트
                    let cloned_cond_elem = cond_elem.clone();
                    let mut guarded_bool = cloned_cond_elem.0.lock().unwrap();
                    *guarded_bool = false;
                    cloned_cond_elem.1.notify_all();

                    res
                }
            }
        }
    }
}

/*

        // match value {
        //     Entry::Occupied(mutex_value) => {
        //         mutex_value.get().clone()
        //     }
        //     Entry::Vacant(mutex_value) => {
        //         let val = self.cond_per_key.get(&arc_key.clone());
        //
        //         match val {
        //             Some(handle) => {
        //                 let mut_and_cond = handle.clone();
        //                 let cloned = mut_and_cond.clone();
        //                 let mut waiting = cloned.0.lock().unwrap();
        //
        //                 while *waiting {
        //                     waiting = mut_and_cond.clone().1.wait(cloned.0.lock().unwrap()).unwrap();
        //                 }
        //                 inner.get(&arc_key.clone()).unwrap().clone()
        //             }
        //             None => {
        //                 let is_cond_inserted = self.cond_per_key.get(&arc_key.clone());
        //                 match is_cond_inserted {
        //                     Some(handle) => {
        //                         let mut_and_cond = handle.clone();
        //                         let cloned = mut_and_cond.clone();
        //                         let mut waiting = cloned.0.lock().unwrap();
        //
        //                         while *waiting {
        //                             waiting = cloned.1.wait(cloned.0.lock().unwrap()).unwrap();
        //                         }
        //                         inner.get(&arc_key.clone()).unwrap().clone()
        //                     }
        //                     None => {
        //                         let flag = Arc::new((Mutex::new(true), Condvar::new()));
        //                         let mut mut_and_cond = self.cond_per_key.clone();
        //                         let res = mut_and_cond.insert(arc_key.clone(), flag).unwrap();
        //
        //                         let calculated = f((*arc_key).clone());
        //                         inner.insert(arc_key, calculated.clone());
        //
        //                         let cond_var = &res.clone().1;
        //                         *(res.clone().0.lock().unwrap()) = false;
        //                         cond_var.notify_all();
        //
        //                         calculated
        //                     }
        //                 }
        //             }
        //         }
        //     }
        // }
    // pub fn get_or_insert_with<F: FnOnce(K) -> V>(&self, key: K, f: F) -> V {
    //     let inner = &self.inner;
    //     let arc_key = Arc::new(key);
    //     let value = inner.entry(arc_key.clone());
    //
    //     match value {
    //         Entry::Occupied(mutex_value) => {
    //             mutex_value.get()
    //         }
    //         Entry::Vacant(mutex_value) => {
    //             let val = self.cond_per_key.get(&arc_key.clone());
    //
    //             match val {
    //                 Some(handle) => {
    //                     let mut_and_cond = handle.clone();
    //                     let mut waiting = mut_and_cond.clone().0.lock().unwrap();
    //
    //                     while *waiting {
    //                         waiting = mut_and_cond.clone().1.wait(mut_and_cond.clone().0.lock().unwrap()).unwrap();
    //                     }
    //                     return inner.get(&arc_key.clone()).unwrap().clone();
    //                 } None => {
    //                     let is_cond_inserted = self.cond_per_key.get(&arc_key.clone());
    //                     match is_cond_inserted {
    //                         Some(handle) => {
    //                             let mut_and_cond = handle.clone();
    //                             let mut waiting = mut_and_cond.clone().0.lock().unwrap();
    //
    //                             while *waiting {
    //                                 waiting = mut_and_cond.clone().1.wait(mut_and_cond.clone().0.lock().unwrap()).unwrap();
    //                             }
    //                             inner.get(&arc_key.clone())
    //                         } None => {
    //                             let flag = Arc::new((Mutex::new(true), Condvar::new()));
    //                             let mut_and_cond = self.cond_per_key.clone();
    //
    //                             let calculated = f((*arc_key).clone());
    //                             inner.insert(arc_key, calculated.clone());
    //
    //                             let cloned = flag.clone();
    //                             let cond_var = cloned.clone().1;
    //                             *cloned.0.lock().unwrap() = false;
    //                             cond_var.notify_all();
    //
    //                             calculated
    //                         }
    //                     }
    //                 }
    //             }
    //         }
    //     };
*/
