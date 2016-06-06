use taxonomy::util::Id as TaxoId;

use std::sync::{ Arc, RwLock };

#[derive(Debug, Clone)]
pub struct IdMap<Kind, Type> {
    map: Arc<RwLock<Vec<(TaxoId<Kind>, Type)>>>
}

impl<Kind, Type> IdMap<Kind, Type> where Type: Eq + Clone, Kind: Clone {
    pub fn new() -> Self {
        IdMap {
            map: Arc::new(RwLock::new(Vec::new()))
        }
    }

    pub fn push(&mut self, id: TaxoId<Kind>, ozw_object: Type) {
        let mut guard = self.map.write().unwrap(); // we have bigger problems if we're poisoned
        guard.push((id, ozw_object));
    }

    pub fn find_taxo_id_from_ozw(&self, needle: &Type) -> Option<TaxoId<Kind>> {
        let guard = self.map.read().unwrap(); // we have bigger problems if we're poisoned
        let find_result = guard.iter().find(|&&(_, ref item)| item == needle);
        find_result.map(|&(ref id, _)| id.clone())
    }

    pub fn find_ozw_from_taxo_id(&self, needle: &TaxoId<Kind>) -> Option<Type> {
        let guard = self.map.read().unwrap(); // we have bigger problems if we're poisoned
        let find_result = guard.iter().find(|&&(ref id, _)| id == needle);
        find_result.map(|&(_, ref ozw_object)| ozw_object.clone())
    }

    pub fn remove_by_ozw(&mut self, needle: &Type) -> Option<TaxoId<Kind>> {
        let mut guard = self.map.write().unwrap(); // we have bigger problems if we're poisoned
        guard.iter().position(|&(_, ref item)| item == needle).map(|index| guard.remove(index).0)
    }
}
