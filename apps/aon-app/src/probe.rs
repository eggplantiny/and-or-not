use crate::cell_buffer::TextPanel;
use aon_sim::{
    LogicLevel, Revision, SignalArrivalKind, SignalProbeTarget, SignalProbeValue, Simulation,
    StepReport, Tick,
};
use std::collections::{BTreeMap, VecDeque};
use thiserror::Error;

pub use aon_sim::SignalProbeTarget as ProbeTarget;

pub const MAX_SIGNAL_PROBES: usize = 8;
pub const PROBE_HISTORY_TICKS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProbeId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArrivalMarkerBand {
    pub propagation: bool,
    pub topology_sync: bool,
}

impl ArrivalMarkerBand {
    pub const fn token(self) -> &'static str {
        match (self.propagation, self.topology_sync) {
            (false, false) => "·",
            (true, false) => "A",
            (false, true) => "S",
            (true, true) => "AS",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArrivalMarkerSample {
    pub completed_tick: Tick,
    pub band: ArrivalMarkerBand,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProbeMarkers {
    pub driver_revision: Option<Revision>,
    pub arrival_band: ArrivalMarkerBand,
    pub target_arrival: bool,
}

impl ProbeMarkers {
    pub fn revision_token(self) -> Option<String> {
        self.driver_revision
            .map(|revision| format!("r{}", revision.0))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProbeSample {
    pub completed_tick: Tick,
    pub next_tick: Tick,
    pub value: SignalProbeValue,
    pub markers: ProbeMarkers,
}

impl ProbeSample {
    pub const fn logic_level(self) -> LogicLevel {
        match self.value {
            SignalProbeValue::Driver(sample) => sample.level,
            SignalProbeValue::Sink { level, .. } => level,
            SignalProbeValue::Wire { active_level, .. } => active_level,
        }
    }

    pub const fn logic_glyph(self) -> char {
        match self.logic_level() {
            LogicLevel::Low => '0',
            LogicLevel::High => '1',
            LogicLevel::X => 'X',
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeTrace {
    target: SignalProbeTarget,
    history: VecDeque<ProbeSample>,
    live: bool,
}

impl ProbeTrace {
    pub const fn target(&self) -> SignalProbeTarget {
        self.target
    }

    pub fn history(&self) -> &VecDeque<ProbeSample> {
        &self.history
    }

    pub fn latest(&self) -> Option<ProbeSample> {
        self.history.back().copied()
    }

    pub const fn is_live(&self) -> bool {
        self.live
    }

    fn push(&mut self, sample: ProbeSample) {
        if self.history.len() == PROBE_HISTORY_TICKS {
            self.history.pop_front();
        }
        self.history.push_back(sample);
        self.live = true;
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProbeRack {
    next_id: u64,
    traces: BTreeMap<ProbeId, ProbeTrace>,
    arrival_history: VecDeque<ArrivalMarkerSample>,
}

impl ProbeRack {
    pub fn add(&mut self, target: SignalProbeTarget) -> Result<ProbeId, ProbeError> {
        if let Some((&id, _)) = self.traces.iter().find(|(_, trace)| trace.target == target) {
            return Ok(id);
        }
        if self.traces.len() == MAX_SIGNAL_PROBES {
            return Err(ProbeError::ProbeLimitReached {
                limit: MAX_SIGNAL_PROBES,
            });
        }
        let id = ProbeId(self.next_id);
        let next_id = self
            .next_id
            .checked_add(1)
            .ok_or(ProbeError::ProbeIdExhausted)?;
        self.traces.insert(
            id,
            ProbeTrace {
                target,
                history: VecDeque::with_capacity(PROBE_HISTORY_TICKS),
                live: true,
            },
        );
        self.next_id = next_id;
        Ok(id)
    }

    pub fn add_validated(
        &mut self,
        simulation: &Simulation,
        target: SignalProbeTarget,
    ) -> Result<ProbeId, ProbeError> {
        if simulation.signal_probe(target).is_none() {
            return Err(ProbeError::UnknownTarget);
        }
        self.add(target)
    }

    pub fn remove(&mut self, id: ProbeId) -> bool {
        self.traces.remove(&id).is_some()
    }

    pub fn clear(&mut self) {
        self.traces.clear();
        self.arrival_history.clear();
        self.next_id = 0;
    }

    pub fn trace(&self, id: ProbeId) -> Option<&ProbeTrace> {
        self.traces.get(&id)
    }

    pub fn traces(&self) -> impl Iterator<Item = (ProbeId, &ProbeTrace)> {
        self.traces.iter().map(|(&id, trace)| (id, trace))
    }

    pub fn arrival_history(&self) -> &VecDeque<ArrivalMarkerSample> {
        &self.arrival_history
    }

    pub fn record_step(&mut self, simulation: &Simulation, report: &StepReport) {
        let arrival_band = ArrivalMarkerBand {
            propagation: report
                .signal_arrivals
                .iter()
                .any(|arrival| arrival.kind == SignalArrivalKind::Propagation),
            topology_sync: report
                .signal_arrivals
                .iter()
                .any(|arrival| arrival.kind == SignalArrivalKind::TopologySync),
        };
        if self.arrival_history.len() == PROBE_HISTORY_TICKS {
            self.arrival_history.pop_front();
        }
        self.arrival_history.push_back(ArrivalMarkerSample {
            completed_tick: report.completed_tick,
            band: arrival_band,
        });

        for trace in self.traces.values_mut() {
            let Some(probe) = simulation.signal_probe(trace.target) else {
                trace.live = false;
                continue;
            };
            let driver_revision = revision_marker(trace.latest(), probe.value);
            let target_arrival = report
                .signal_arrivals
                .iter()
                .any(|arrival| arrival_matches(trace.target, probe.value, arrival));
            trace.push(ProbeSample {
                completed_tick: report.completed_tick,
                next_tick: report.next_tick,
                value: probe.value,
                markers: ProbeMarkers {
                    driver_revision,
                    arrival_band,
                    target_arrival,
                },
            });
        }
    }

    pub fn waveform_panel(&self, visible_ticks: usize) -> TextPanel {
        let visible_ticks = visible_ticks.min(PROBE_HISTORY_TICKS);
        let mut lines = Vec::new();
        let arrival_tokens = self
            .arrival_history
            .iter()
            .rev()
            .take(visible_ticks)
            .rev()
            .map(|sample| sample.band.token())
            .collect::<Vec<_>>()
            .join(" ");
        lines.push(format!("arrivals {arrival_tokens}"));

        for (id, trace) in self.traces() {
            let samples = trace
                .history()
                .iter()
                .rev()
                .take(visible_ticks)
                .rev()
                .copied()
                .collect::<Vec<_>>();
            let values = samples
                .iter()
                .map(|sample| sample.logic_glyph())
                .collect::<String>();
            let revisions = samples
                .iter()
                .filter_map(|sample| sample.markers.revision_token())
                .collect::<Vec<_>>()
                .join(" ");
            let target_markers = samples
                .iter()
                .map(|sample| {
                    if sample.markers.target_arrival {
                        '^'
                    } else {
                        '·'
                    }
                })
                .collect::<String>();
            lines.push(format!(
                "P{} {:<12} {values}",
                id.0,
                target_label(trace.target)
            ));
            lines.push(format!("   revisions   {revisions}"));
            lines.push(format!("   target      {target_markers}"));
        }
        TextPanel::new(format!("Waveform last {visible_ticks}"), lines)
    }

    pub fn inspector_panel(&self, id: ProbeId) -> Option<TextPanel> {
        let trace = self.trace(id)?;
        let mut lines = vec![format!("target={}", target_label(trace.target))];
        if let Some(sample) = trace.latest() {
            lines.push(format!("completed_tick={}", sample.completed_tick.0));
            lines.push(signal_value_text(sample.value));
            lines.push(format!(
                "revision={} arrivals={} target_match={}",
                sample
                    .markers
                    .revision_token()
                    .unwrap_or_else(|| "-".to_owned()),
                sample.markers.arrival_band.token(),
                sample.markers.target_arrival
            ));
        } else if trace.live {
            lines.push("no samples".to_owned());
        } else {
            lines.push("target removed".to_owned());
        }
        Some(TextPanel::new(format!("Inspector P{}", id.0), lines))
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ProbeError {
    #[error("signal probe limit {limit} reached")]
    ProbeLimitReached { limit: usize },

    #[error("signal probe target is unknown or removed")]
    UnknownTarget,

    #[error("host ProbeId space is exhausted")]
    ProbeIdExhausted,
}

fn revision_marker(previous: Option<ProbeSample>, value: SignalProbeValue) -> Option<Revision> {
    let SignalProbeValue::Driver(current) = value else {
        return None;
    };
    let previous_revision = previous.and_then(|sample| match sample.value {
        SignalProbeValue::Driver(previous) => Some(previous.revision),
        SignalProbeValue::Sink { .. } | SignalProbeValue::Wire { .. } => None,
    });
    (previous_revision != Some(current.revision)).then_some(current.revision)
}

fn arrival_matches(
    target: SignalProbeTarget,
    value: SignalProbeValue,
    arrival: &aon_sim::SignalArrivalObservation,
) -> bool {
    match target {
        SignalProbeTarget::Driver(driver) => arrival.source_driver == driver,
        SignalProbeTarget::Sink(sink) => arrival.sink == sink,
        SignalProbeTarget::Wire(_) => false,
        SignalProbeTarget::GateInputA(_) | SignalProbeTarget::GateInputB(_) => {
            matches!(value, SignalProbeValue::Sink { sink, .. } if arrival.sink == sink)
        }
        SignalProbeTarget::GateOutput(_) => {
            matches!(value, SignalProbeValue::Driver(sample) if arrival.source_driver == sample.driver_id)
        }
    }
}

fn target_label(target: SignalProbeTarget) -> String {
    match target {
        SignalProbeTarget::Driver(id) => format!("driver:{}", id.0.0),
        SignalProbeTarget::Sink(id) => format!("sink:{}", id.0.0),
        SignalProbeTarget::Wire(id) => format!("wire:{}", id.0.0),
        SignalProbeTarget::GateInputA(id) => format!("gate:{}:in-a", id.0.0),
        SignalProbeTarget::GateInputB(id) => format!("gate:{}:in-b", id.0.0),
        SignalProbeTarget::GateOutput(id) => format!("gate:{}:out", id.0.0),
    }
}

fn signal_value_text(value: SignalProbeValue) -> String {
    match value {
        SignalProbeValue::Driver(sample) => format!(
            "logic={} strength={} revision={} emitted={}",
            logic_name(sample.level),
            sample.strength.0,
            sample.revision.0,
            sample.emitted_at.0
        ),
        SignalProbeValue::Sink { sink, level } => {
            format!("sink={} logic={}", sink.0.0, logic_name(level))
        }
        SignalProbeValue::Wire {
            active_drive,
            previous_drive,
            active_level,
            previous_level,
        } => format!(
            "logic={} previous={} drive=H{} L{} X{} previous_drive=H{} L{} X{}",
            logic_name(active_level),
            logic_name(previous_level),
            active_drive.high,
            active_drive.low,
            active_drive.unknown,
            previous_drive.high,
            previous_drive.low,
            previous_drive.unknown
        ),
    }
}

const fn logic_name(level: LogicLevel) -> &'static str {
    match level {
        LogicLevel::Low => "LOW",
        LogicLevel::High => "HIGH",
        LogicLevel::X => "X",
    }
}
