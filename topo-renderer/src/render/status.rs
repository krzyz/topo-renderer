use std::collections::{HashMap, HashSet};

use topo_common::GeoLocation;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PendingOperation {
    FetchingTerrain,
    ProcessingTerrain,
    WritingTerrain,
    PreparingLabels,
    LoadingFonts,
}

pub struct StatusNotifier {
    pending_operations: HashMap<Option<GeoLocation>, HashSet<PendingOperation>>,
    listeners: Vec<Box<dyn Fn(&str)>>,
}

impl StatusNotifier {
    pub fn new() -> Self {
        Self {
            pending_operations: HashMap::new(),
            listeners: vec![],
        }
    }
    pub fn add_listener(&mut self, listener: Box<dyn Fn(&str)>) {
        self.listeners.push(listener);
    }

    pub fn status_string(&self) -> String {
        let num_pending: usize = self
            .pending_operations
            .iter()
            .map(|(_, ops)| ops.len())
            .sum();

        if num_pending > 0 {
            format!("Pending: {num_pending}")
        } else {
            format!("")
        }
    }

    pub fn update_pending_operations(
        &mut self,
        location: Option<GeoLocation>,
        to_add: &[PendingOperation],
        to_remove: &[PendingOperation],
    ) {
        let set_for_location = self
            .pending_operations
            .entry(location)
            .or_insert(HashSet::new());
        for pending_op in to_remove {
            set_for_location.remove(&pending_op);
        }
        for pending_op in to_add {
            set_for_location.insert(*pending_op);
        }

        let status = self.status_string();

        for listener in self.listeners.iter_mut() {
            listener(status.as_str());
        }
    }
}
