//! Veyran doubles triggered abilities of permanents, not spell abilities.

use engine::game::scenario::{GameScenario, P0};
use engine::types::game_state::{StackEntryKind, SyntheticTriggerProvenance};
use engine::types::mana::ManaCost;
use engine::types::phase::Phase;

const VEYRAN_DOUBLER_ORACLE: &str = "If you casting or copying an instant or sorcery spell causes a triggered ability of a permanent you control to trigger, that ability triggers an additional time.";
const CHATTERSTORM_ORACLE: &str = "Convoke\n\
Create a 1/1 green Squirrel creature token.\n\
Storm (When you cast this spell, copy it for each spell cast before it this turn. You may choose new targets for the copies.)";

/// CR 603.2d + CR 702.40a: Veyran's "ability of a permanent" scope excludes
/// Storm, a triggered ability of the spell on the stack.
#[test]
fn veyran_does_not_double_storm() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);
    scenario.add_creature_from_oracle(P0, "Veyran, Voice of Duality", 2, 2, VEYRAN_DOUBLER_ORACLE);
    let chatterstorm = scenario
        .add_spell_to_hand_from_oracle(P0, "Chatterstorm", false, CHATTERSTORM_ORACLE)
        .with_mana_cost(ManaCost::zero())
        .id();

    let mut runner = scenario.build();
    let commit = runner.cast(chatterstorm).commit();
    let state = commit.state();
    let storm_triggers = state
        .stack
        .iter()
        .filter(|entry| {
            matches!(
                &entry.kind,
                StackEntryKind::TriggeredAbility {
                    provenance: Some(SyntheticTriggerProvenance::Storm { .. }),
                    ..
                }
            )
        })
        .count();

    assert_eq!(
        storm_triggers, 1,
        "Veyran must not double Storm because Storm belongs to the spell, not a permanent"
    );
}
