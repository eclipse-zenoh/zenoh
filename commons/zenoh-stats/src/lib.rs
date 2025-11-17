use std::fmt;

mod histogram;
mod labels;
mod link;
mod per_remote;
mod registry;
mod stats;
mod transport;

pub use crate::{
    labels::{LocalityLabel, ReasonLabel, ResourceLabel},
    link::{rx_set_space, tx_with_space, LinkStats},
    registry::StatsRegistry,
    transport::{DropStats, TransportStats},
    StatsDirection::*,
};

#[derive(Debug, Clone, Copy)]
pub enum StatsDirection {
    Tx,
    Rx,
}

impl StatsDirection {
    const NUM: usize = 2;

    fn from_index(index: usize) -> Self {
        match index {
            i if i == Tx as usize => Tx,
            i if i == Rx as usize => Rx,
            _ => unreachable!(),
        }
    }
}

impl fmt::Display for StatsDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tx => write!(f, "tx"),
            Self::Rx => write!(f, "rx"),
        }
    }
}
