use std::collections::HashMap;

use color_eyre::eyre::Error;
use futures::FutureExt;
use tokio_with_wasm::alias as tokio;
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
    pending_operations: HashMap<
        Option<GeoLocation>,
        HashMap<PendingOperation, tokio::task::JoinHandle<Result<(), Error>>>,
    >,
    listeners: Vec<Box<dyn Fn(&str)>>,
}

impl StatusNotifier {
    pub fn new() -> Self {
        Self {
            pending_operations: HashMap::new(),
            listeners: vec![],
        }
    }

    pub fn check_progress(&mut self) {
        let mut to_remove = vec![];
        for (location, operation_map) in &self.pending_operations {
            for (operation, handle) in operation_map {
                if handle.is_finished() {
                    to_remove.push((*location, *operation));
                }
            }
        }

        for (location, operation) in to_remove {
            self.pending_operations
                .entry(location)
                .and_modify(|operations_map| {
                    if let Some(handle) = operations_map.remove(&operation)
                        && let Some(result) = handle.now_or_never()
                        && let Err(err) = result
                    {
                        log::error!("{err}");
                    };
                });
        }

        let status = self.status_string();

        for listener in self.listeners.iter_mut() {
            listener(status.as_str());
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
        to_add: Vec<(PendingOperation, tokio::task::JoinHandle<Result<(), Error>>)>,
    ) {
        let set_for_location = self
            .pending_operations
            .entry(location)
            .or_insert(HashMap::new());
        for (pending_op, join_handle) in to_add {
            set_for_location.insert(pending_op, join_handle);
        }

        let status = self.status_string();

        for listener in self.listeners.iter_mut() {
            listener(status.as_str());
        }
    }
}
