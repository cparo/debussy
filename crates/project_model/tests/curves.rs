//! Automation curves in the project: curve lanes, curve ports and control wires, each edit undone
//! exactly (`docs/architecture/automation.md` "Curves in the project").

use std::collections::BTreeMap;
use std::error::Error as StdError;

use modulation::{Curve, CurveError, Handle, Point};
use project_model::{Edit, Endpoint, Error, NodeId, NodeKind, Session};

type TestResult = Result<(), Box<dyn StdError>>;

fn mixer(session: &mut Session, channels: usize) -> Result<NodeId, Error> {
    session.add_node(NodeKind::Mixer {
        channels,
        parameters: BTreeMap::new(),
    })
}

fn effect(session: &mut Session, kind: &str) -> Result<NodeId, Error> {
    session.add_node(NodeKind::Effect {
        kind: kind.to_owned(),
        parameters: BTreeMap::new(),
    })
}

fn flat() -> Curve {
    Curve::new(vec![Point::new(0, 0.5)]).unwrap_or_default()
}

#[test]
fn test_add_rename_and_remove_curve_undo_exactly() -> TestResult {
    let mut session = Session::new();
    let before = session.state().clone();
    let curve = session.add_curve("Curve 1", flat())?;
    assert_eq!(session.state().curves.len(), 1);
    session.rename_curve(curve, "Swell")?;
    assert_eq!(
        session.state().curve(curve).map(|l| l.name.as_str()),
        Some("Swell")
    );
    session.remove_curve(curve)?;
    assert!(session.state().curves.is_empty());
    assert!(session.undo() && session.undo());
    assert_eq!(
        session.state().curve(curve).map(|l| l.name.as_str()),
        Some("Curve 1")
    );
    assert!(session.undo());
    assert_eq!(session.state(), &before);
    assert_eq!(session.remove_curve(curve), Err(Error::UnknownCurve(curve)));
    Ok(())
}

#[test]
fn test_set_curve_refuses_points_that_turn_back_in_time() -> TestResult {
    let mut session = Session::new();
    let curve = session.add_curve("Curve 1", flat())?;
    let good = vec![
        Point::new(0, 0.0).with_handles(Handle::default(), Handle::new(100.0, 0.0)),
        Point::new(300, 1.0).with_handles(Handle::new(-100.0, 0.0), Handle::default()),
    ];
    session.set_curve(curve, good.clone())?;
    let mut bad = good.clone();
    bad[0].out_handle = Handle::new(-5.0, 0.0);
    assert_eq!(
        session.set_curve(curve, bad),
        Err(Error::InvalidCurve(CurveError::NotFunctionOfTime {
            segment: 0
        }))
    );
    assert_eq!(
        session
            .state()
            .curve(curve)
            .map(|l| l.curve.points().to_vec()),
        Some(good)
    );
    assert!(session.undo());
    assert_eq!(
        session.state().curve(curve).map(|l| l.curve.clone()),
        Some(flat())
    );
    Ok(())
}

#[test]
fn test_assign_curve_port_takes_the_lowest_free_number() -> TestResult {
    let mut session = Session::new();
    let node = mixer(&mut session, 2)?;
    assert_eq!(session.assign_curve_port(node, "channel 1 gain")?, 1);
    assert_eq!(session.assign_curve_port(node, "channel 2 pan")?, 2);
    assert_eq!(session.assign_curve_port(node, "channel 2 gain")?, 3);
    session.unassign_curve_port(node, "channel 2 pan")?;
    assert_eq!(session.assign_curve_port(node, "channel 1 pan")?, 2);
    assert_eq!(
        session.assign_curve_port(node, "channel 1 pan"),
        Err(Error::AlreadyAssigned(node, "channel 1 pan".into()))
    );
    // Undo puts the old number back exactly.
    assert!(session.undo() && session.undo());
    let ports = &session.state().node(node).ok_or("no node")?.curve_ports;
    assert_eq!(ports.get("channel 2 pan"), Some(&2));
    Ok(())
}

#[test]
fn test_assign_curve_port_refuses_parameters_that_cannot_be_automated() -> TestResult {
    let mut session = Session::new();
    let node = mixer(&mut session, 1)?;
    let filter = effect(&mut session, "high-pass")?;
    let unknown = effect(&mut session, "eq")?;
    let bus = session.add_node(NodeKind::Bus(audio_buffers::Layout::Mono))?;
    let refused = [
        (node, "channel 1 mode"), // a choice
        (node, "channel 2 gain"), // no such channel
        (filter, "order"),        // stopped-only and whole numbers
        (filter, "no such"),      // no such parameter
        (unknown, "frequency"),   // an effect this build does not know
        (bus, "gain"),            // a node with no parameters
    ];
    for (target, name) in refused {
        assert_eq!(
            session.assign_curve_port(target, name),
            Err(Error::NotAutomatable(target, name.into())),
            "{name}"
        );
    }
    // A logarithmic parameter may be automated.
    assert_eq!(session.assign_curve_port(filter, "cutoff")?, 1);
    let port = Edit::AssignCurvePort {
        node,
        parameter: "channel 1 gain".into(),
        port: 0,
    };
    assert_eq!(session.apply(port), Err(Error::CurvePortTaken(node, 0)));
    Ok(())
}

#[test]
fn test_curve_connects_only_to_a_curve_port_with_one_wire_each() -> TestResult {
    let mut session = Session::new();
    let node = mixer(&mut session, 2)?;
    let curve = session.add_curve("Curve 1", flat())?;
    let other = session.add_curve("Curve 2", flat())?;
    let gain = session.assign_curve_port(node, "channel 1 gain")?;
    let pan = session.assign_curve_port(node, "channel 2 pan")?;
    session.connect_curve(curve, node, gain)?;
    // One curve may drive many ports.
    session.connect_curve(curve, node, pan)?;
    assert_eq!(session.state().curve_into(node, gain), Some(curve));
    assert_eq!(
        session.connect_curve(other, node, gain),
        Err(Error::PortTaken(Endpoint::CurvePort(node, gain)))
    );
    assert_eq!(
        session.connect_curve(curve, node, 9),
        Err(Error::NoSuchPort(Endpoint::CurvePort(node, 9)))
    );
    assert_eq!(
        session.connect(Endpoint::Curve(curve), 0, Endpoint::Node(node), 0),
        Err(Error::WrongPortType)
    );
    assert_eq!(
        session.connect(Endpoint::Node(node), 0, Endpoint::CurvePort(node, gain), 0),
        Err(Error::WrongPortType)
    );
    assert_eq!(
        session.connect(
            Endpoint::CurvePort(node, gain),
            0,
            Endpoint::Curve(curve),
            0
        ),
        Err(Error::NoSuchPort(Endpoint::CurvePort(node, gain)))
    );
    Ok(())
}

#[test]
fn test_removing_ends_removes_their_wires_in_one_step() -> TestResult {
    let mut session = Session::new();
    let node = mixer(&mut session, 1)?;
    let curve = session.add_curve("Curve 1", flat())?;
    let port = session.assign_curve_port(node, "channel 1 gain")?;
    session.connect_curve(curve, node, port)?;
    let wired = session.state().clone();

    session.unassign_curve_port(node, "channel 1 gain")?;
    assert!(session.state().connections.is_empty());
    assert!(session.undo());
    assert_eq!(session.state(), &wired);

    session.remove_curve(curve)?;
    assert!(session.state().connections.is_empty());
    assert!(session.undo());
    assert_eq!(session.state(), &wired);

    session.remove_node(node)?;
    assert!(session.state().connections.is_empty());
    assert!(session.undo());
    assert_eq!(session.state(), &wired);

    // The bare edits refuse while wired.
    assert_eq!(
        session.apply(Edit::RemoveCurve(curve)),
        Err(Error::StillConnected(Endpoint::Curve(curve)))
    );
    assert_eq!(
        session.apply(Edit::UnassignCurvePort {
            node,
            parameter: "channel 1 gain".into()
        }),
        Err(Error::StillConnected(Endpoint::CurvePort(node, port)))
    );
    Ok(())
}

#[test]
fn test_fewer_mixer_channels_refused_while_a_removed_parameter_has_a_port() -> TestResult {
    let mut session = Session::new();
    let node = mixer(&mut session, 1)?;
    session.add_mixer_channels(node, 1)?;
    session.assign_curve_port(node, "channel 2 gain")?;
    assert_eq!(
        session.apply(Edit::SetMixerChannels { node, channels: 1 }),
        Err(Error::StillAssigned(node, "channel 2 gain".into()))
    );
    Ok(())
}

#[test]
fn test_restore_checks_curve_ports_and_open_takes_curve_ids() -> TestResult {
    let mut session = Session::new();
    let node = mixer(&mut session, 1)?;
    let curve = session.add_curve("Curve 1", flat())?;
    let mut bad = session.state().clone();
    bad.nodes[0].curve_ports.insert("channel 1 mode".into(), 1);
    assert_eq!(
        session.apply(Edit::Restore(bad)),
        Err(Error::NotAutomatable(node, "channel 1 mode".into()))
    );
    let mut reopened = Session::open(session.state().clone(), BTreeMap::new());
    let next = reopened.add_curve("Curve 2", flat())?;
    assert!(next.0 > curve.0);
    Ok(())
}
