//! Read-only render view, test doubles and headless frame rendering.
//!
//! [`RenderView`] is the wide read-only interface between state and
//! drawing: preparation and drawing only see this trait, never concrete
//! controllers. [`TestRenderModel`] is the test double (fixed history plus
//! streaming text stepped by the injectable clock), [`render_headless`]
//! runs the scrollback layout without a terminal, and
//! [`draw_recovered_frame`] renders the panic fallback so a failing widget
//! degrades to a visible frame instead of killing the loop. Event
//! recording hooks ([`EventRecorder`] / [`EventPlayer`]) serialize an
//! interaction sequence for replay; full capture arrives later.
//!
//! The submodules group the responsibilities: `view` holds the read-only
//! traits, `test_model` the test double, `headless_layout` the terminal-free
//! wrapping and baseline replay, `geometry` the layout probes, `recovery`
//! the panic fallback, `event_recording` the replay sequence, `evidence` the
//! failure bundle and `transport` the frame submit boundary.

mod event_recording;
mod evidence;
mod geometry;
mod headless_layout;
mod recovery;
mod test_model;
mod transport;
mod view;

pub use event_recording::{EventPlayer, EventRecorder, RecordedEvent};
pub use evidence::TestEvidence;
pub use geometry::{
    probe_layout, probe_unified_geometry, FrameGeometry, LayoutProbe, UnifiedGeometryProbe,
};
pub use headless_layout::{
    fixed_baseline_sequence, render_headless, render_headless_document, replay_baseline,
    viewport_window,
};
pub use recovery::{draw_recovered_frame, isolated_component_draw};
pub use test_model::TestRenderModel;
pub use transport::{FrameTransport, TestTransport};
pub use view::{
    InputView, LayoutView, PerfView, RenderView, ScrollView, ThemeView, TranscriptView,
};

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    use super::{
        draw_recovered_frame, fixed_baseline_sequence, probe_layout, probe_unified_geometry,
        render_headless, render_headless_document, replay_baseline, viewport_window, EventPlayer,
        EventRecorder, FrameGeometry, FrameTransport, RecordedEvent, RenderView, TestEvidence,
        TestRenderModel, TestTransport,
    };

    #[test]
    fn headless_render_follows_tail_and_scroll() {
        let mut model = TestRenderModel::new(10);
        for line in ["one", "two", "three", "four"] {
            model.push_history(line);
        }
        let tail = render_headless(&model, 10, 2);
        assert_eq!(tail, vec!["three".to_string(), "four".to_string()]);
        model.scroll = 2;
        let scrolled = render_headless(&model, 10, 2);
        assert_eq!(scrolled, vec!["one".to_string(), "two".to_string()]);
    }

    #[test]
    fn headless_render_includes_streaming_tail() {
        let mut model = TestRenderModel::new(80);
        model.push_history("done");
        model.set_streaming("live…");
        let rows = render_headless(&model, 80, 5);
        assert_eq!(rows, vec!["done".to_string(), "live…".to_string()]);
    }

    #[test]
    fn anim_tick_uses_discrete_buckets() {
        let mut model = TestRenderModel::new(80);
        model.now_ms = 50;
        let first = model.anim_tick();
        model.now_ms = 99;
        assert_eq!(model.anim_tick(), first);
        model.now_ms = 100;
        assert_ne!(model.anim_tick(), first);
    }

    #[test]
    fn viewport_window_clamps_scroll() {
        assert_eq!(viewport_window(10, 4, 0), (6, 10));
        assert_eq!(viewport_window(10, 4, 2), (4, 8));
        assert_eq!(viewport_window(10, 4, 99), (0, 4));
        assert_eq!(viewport_window(2, 4, 0), (0, 2));
    }

    #[test]
    fn geometry_carries_animation_bounds() {
        use crate::redraw::AnimationArea;
        let area = AnimationArea::new(Some(Rect::new(0, 7, 1, 1)), None);
        let geometry = FrameGeometry::split(Rect::new(0, 0, 80, 24), 4, 1).with_anim_bounds(area);
        assert_eq!(geometry.anim_bounds, Some(Rect::new(0, 7, 1, 1)));
        let empty = FrameGeometry::split(Rect::new(0, 0, 80, 24), 4, 1)
            .with_anim_bounds(AnimationArea::default());
        assert_eq!(empty.anim_bounds, None);
    }

    #[test]
    fn recovered_frame_marks_the_buffer() {
        let area = Rect::new(0, 0, 40, 3);
        let mut buf = Buffer::empty(area);
        draw_recovered_frame(&mut buf, area);
        let text: String = buf.content.iter().map(|c| c.symbol().to_string()).collect();
        assert!(text.contains("recovered"));
    }

    #[test]
    fn event_recorder_round_trips_through_json() {
        let mut recorder = EventRecorder::new();
        recorder.push(RecordedEvent::key("enter"));
        recorder.push(RecordedEvent::delta("hi"));
        recorder.push(RecordedEvent::Tick(100));
        let mut player = EventPlayer::from_json(&recorder.to_json());
        assert_eq!(player.next_event(), Some(&RecordedEvent::key("enter")));
        assert_eq!(player.next_event(), Some(&RecordedEvent::delta("hi")));
        assert_eq!(player.next_event(), Some(&RecordedEvent::Tick(100)));
        assert!(player.exhausted());
    }

    #[test]
    fn baseline_replay_is_deterministic() {
        let mut first = TestRenderModel::new(20);
        first.push_history("alpha line one");
        first.push_history("beta line two");
        let mut second = first.clone();
        second.set_streaming("live tail");
        let mut third = second.clone();
        third.scroll = 1;
        let models = [first, second, third];
        let once = replay_baseline(&models, 20, 4);
        let twice = replay_baseline(&models, 20, 4);
        assert_eq!(once.len(), 3);
        assert_eq!(once.report(), twice.report());
        assert!(once.report().contains("frames=3"));
        assert!(once.steady_layout_within(4));
    }

    #[test]
    fn fixed_baseline_sequence_is_deterministic() {
        let seq = fixed_baseline_sequence(20);
        assert_eq!(seq.len(), 6);
        let once = replay_baseline(&seq, 20, 4);
        let twice = replay_baseline(&fixed_baseline_sequence(20), 20, 4);
        assert_eq!(once.len(), 6);
        assert_eq!(once.report(), twice.report());
        assert!(once.total_parsed_bytes() > 0);
    }

    #[test]
    fn long_history_baseline_reports_linear_growth() {
        let mut model = TestRenderModel::new(40);
        for i in 0..200 {
            model.push_history(format!("history line {i} with some words"));
        }
        let models = [model];
        let metrics = replay_baseline(&models, 40, 10);
        assert_eq!(metrics.len(), 1);
        assert!(metrics.total_parsed_bytes() > 200 * 10);
        assert!(metrics.report().contains("frames=1"));
    }

    #[test]
    fn view_carries_size_and_overlay_state() {
        let mut model = TestRenderModel::new(80);
        assert_eq!(model.height(), 24);
        assert!(!model.overlay_active());
        model.height = 30;
        model.overlay = true;
        assert_eq!(model.height(), 30);
        assert!(model.overlay_active());
    }

    #[test]
    fn evidence_bundle_summarizes_failures() {
        let mut evidence = TestEvidence::new("case");
        assert!(!evidence.failed());
        evidence.add_event(RecordedEvent::Tick(7));
        evidence.add_frame("row one");
        evidence.add_assertion("viewport mismatch");
        evidence.add_log("draw took 3ms");
        assert!(evidence.failed());
        let summary = evidence.summary();
        assert!(summary.contains("case"));
        assert!(summary.contains("events=1"));
        assert!(summary.contains("frames=1"));
    }

    #[test]
    fn evidence_seal_freezes_mutation() {
        let mut evidence = TestEvidence::new("sealed");
        evidence.add_frame("one");
        let summary = evidence.seal();
        assert!(evidence.is_sealed());
        assert!(summary.contains("sealed"));
        evidence.add_frame("two");
        assert_eq!(evidence.frames.len(), 1);
    }

    #[test]
    fn unified_probe_carries_diagram_and_sidebar() {
        let frame = Rect::new(0, 0, 100, 30);
        let probe = probe_unified_geometry(frame, 4, 1, 30, true, true, false);
        assert!(probe.diagram.is_some());
        assert!(probe.sidebar.is_some());
        assert!(probe.focused);
        assert!(!probe.overlay_active);
        assert_eq!(probe.diagram_requested, 30);
        let none = probe_unified_geometry(frame, 4, 1, 0, false, true, false);
        assert_eq!(none.diagram, None);
        assert_eq!(none.sidebar, None);
    }

    #[test]
    fn layout_probe_splits_frame_geometry() {
        let frame = Rect::new(0, 0, 80, 24);
        let probe = probe_layout(frame, 4, 1);
        assert_eq!(probe.viewport.height, 19);
        assert_eq!(probe.footer.y, 19);
        assert_eq!(probe.footer.height, 4);
        assert_eq!(probe.input.y, 23);
        assert_eq!(probe.input.height, 1);
        assert_eq!(
            probe.viewport.height + probe.footer.height + probe.input.height,
            24
        );
    }

    #[test]
    fn test_transport_records_only_real_frames() {
        let mut transport = TestTransport::new();
        assert!(!transport.submit(crate::redraw::RedrawScope::None));
        assert!(transport.submit(crate::redraw::RedrawScope::AnimationOnly));
        assert!(transport.submit(crate::redraw::RedrawScope::Full));
        assert_eq!(
            transport.submitted(),
            &[
                crate::redraw::RedrawScope::AnimationOnly,
                crate::redraw::RedrawScope::Full,
            ]
        );
    }

    #[test]
    fn document_headless_agrees_with_plain_ground_truth() {
        let mut model = TestRenderModel::new(40);
        model.push_history("hello world");
        model.push_history("second line here");
        model.set_streaming("live tail");
        assert_eq!(
            render_headless(&model, 40, 10),
            render_headless_document(&model, 40, 10)
        );
    }

    #[test]
    fn widened_view_groups_default_cleanly() {
        let model = TestRenderModel::new(80);
        assert!(!model.auto_scroll_paused());
        assert_eq!(model.input_text(), "");
        assert_eq!(model.input_cursor(), 0);
        assert!(!model.is_processing());
        assert_eq!(model.queued_count(), 0);
        assert_eq!(model.perf_marker(), "perf:full");
        assert!(!model.side_panel_visible());
        assert!(!model.diagram_visible());
        assert_eq!(model.image_signature(), 0);
    }

    #[test]
    fn recorded_events_carry_timestamps() {
        let key = RecordedEvent::Key {
            label: "enter".to_string(),
            at_ms: 12,
        };
        assert_eq!(key.at_ms(), Some(12));
        let bus = RecordedEvent::Bus {
            topic: "fetch".to_string(),
            at_ms: 34,
        };
        assert_eq!(bus.at_ms(), Some(34));
    }

    #[test]
    fn anchor_stability_scores_scrolled_frames() {
        use crate::anchor::{AnchorFrame, AnchorStabilityRecorder};
        let mut recorder = AnchorStabilityRecorder::new();
        recorder.record(AnchorFrame::from_rows(&["a".to_string(), "b".to_string()]));
        assert!(recorder.record(AnchorFrame::from_rows(&["a".to_string(), "b".to_string()])));
        assert_eq!(recorder.stability(), 1.0);
    }

    #[test]
    fn wrapped_map_supports_selection_lookup() {
        use crate::anchor::WrappedLineMap;
        let map = WrappedLineMap::build(&[2, 1]);
        assert_eq!(map.logical_for_wrapped(0), Some(0));
        assert_eq!(map.logical_for_wrapped(2), Some(1));
    }
}
