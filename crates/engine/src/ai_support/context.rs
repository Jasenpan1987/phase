use crate::types::game_state::{GameState, WaitingFor};

use crate::game::turn_control;
use crate::types::actions::GameAction;
use crate::types::player::PlayerId;

use super::candidates::CandidateAction;
use super::validated_candidate_actions_for_semantic_owner;

#[derive(Debug, Clone)]
pub struct AiDecisionContext {
    pub waiting_for: WaitingFor,
    pub candidates: Vec<CandidateAction>,
}

/// The finite action domain issued by the engine for one AI decision.
///
/// `WaitingFor` carries the typed bounds for individual choice fields; this
/// contract is the corresponding complete bound for compound actions.  A
/// consumer must submit one of these exact actions, for the semantic owner and
/// authorized actor recorded here, rather than reconstructing an action from
/// partial UI state.
#[derive(Debug, Clone)]
pub struct AiDecisionContract {
    pub semantic_owner: PlayerId,
    pub authorized_actor: PlayerId,
    pub state_revision: u64,
    pub candidates: Vec<CandidateAction>,
}

impl AiDecisionContract {
    pub fn issue(state: &GameState, semantic_owner: PlayerId) -> Self {
        Self {
            semantic_owner,
            authorized_actor: turn_control::authorized_submitter_for_player(state, semantic_owner),
            state_revision: state.state_revision,
            candidates: validated_candidate_actions_for_semantic_owner(state, semantic_owner),
        }
    }

    /// Checks the values that are stable within an engine state. Transport
    /// session/generation invalidation belongs to the authority that mints the
    /// opaque proposal token (WASM/server), because a restored state resets its
    /// serialized revision.
    pub fn permits(&self, state: &GameState, actor: PlayerId, action: &GameAction) -> bool {
        self.state_revision == state.state_revision
            && state
                .waiting_for
                .acting_players()
                .contains(&self.semantic_owner)
            && self.authorized_actor == actor
            && turn_control::authorized_submitter_for_player(state, self.semantic_owner) == actor
            && self
                .candidates
                .iter()
                .any(|candidate| candidate.action == *action)
    }
}

pub fn build_decision_context(state: &GameState) -> AiDecisionContext {
    // The tactical layer must receive the same finite, simulated-legal domain
    // as the action boundary.  Returning raw enumeration here is how a policy
    // could select an action whose arguments were never dispatchable.
    let semantic_owner = state
        .waiting_for
        .acting_player()
        .or_else(|| state.waiting_for.acting_players().first().copied());
    let candidates = semantic_owner.map_or_else(Vec::new, |owner| {
        build_decision_context_for_semantic_owner(state, owner).candidates
    });
    AiDecisionContext {
        waiting_for: state.waiting_for.clone(),
        candidates,
    }
}

/// Build the AI view for one named semantic decision owner. Callers that
/// already know which pending player they are selecting for (simultaneous
/// mulligans and controlled turns) must use this rather than accepting the
/// generic context's first pending owner.
pub fn build_decision_context_for_semantic_owner(
    state: &GameState,
    semantic_owner: PlayerId,
) -> AiDecisionContext {
    AiDecisionContext {
        waiting_for: state.waiting_for.clone(),
        candidates: AiDecisionContract::issue(state, semantic_owner).candidates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::zones::create_object;
    use crate::types::{
        actions::GameAction,
        card_type::CoreType,
        identifiers::{CardId, ObjectId},
        player::PlayerId,
        zones::Zone,
        Phase,
    };

    /// Issue #4878: the decision context is consumed directly by phase-ai, so
    /// it must canonicalize candidate enumeration order before trajectories
    /// score tied actions. The hand deliberately enumerates the two land
    /// actions in descending object-id order; removing the context sort makes
    /// this assertion fail while tests for other candidate consumers still pass.
    #[test]
    fn build_decision_context_canonicalizes_candidate_action_order() {
        let player = PlayerId(0);
        let mut state = GameState::new_two_player(42);
        state.phase = Phase::PreCombatMain;
        state.active_player = player;
        state.priority_player = player;
        state.waiting_for = WaitingFor::Priority { player };

        let first_land = create_object(
            &mut state,
            CardId(1),
            player,
            "First Land".to_string(),
            Zone::Hand,
        );
        let second_land = create_object(
            &mut state,
            CardId(2),
            player,
            "Second Land".to_string(),
            Zone::Hand,
        );
        for object_id in [first_land, second_land] {
            state
                .objects
                .get_mut(&object_id)
                .expect("created land must exist")
                .card_types
                .core_types
                .push(CoreType::Land);
        }
        state.players[0].hand = [second_land, first_land].into_iter().collect();

        let context = build_decision_context(&state);
        let land_actions: Vec<_> = context
            .candidates
            .iter()
            .filter_map(|candidate| match &candidate.action {
                GameAction::PlayLand { object_id, .. } => Some(*object_id),
                _ => None,
            })
            .collect();

        assert_eq!(land_actions, vec![first_land, second_land]);
    }

    #[test]
    fn decision_contract_requires_an_exact_issued_action() {
        let player = PlayerId(0);
        let mut state = GameState::new_two_player(42);
        state.phase = Phase::PreCombatMain;
        state.active_player = player;
        state.priority_player = player;
        state.waiting_for = WaitingFor::Priority { player };
        let land = create_object(
            &mut state,
            CardId(1),
            player,
            "Bounded Land".to_string(),
            Zone::Hand,
        );
        state
            .objects
            .get_mut(&land)
            .expect("created land must exist")
            .card_types
            .core_types
            .push(CoreType::Land);
        state.players[0].hand.push_back(land);

        let contract = AiDecisionContract::issue(&state, player);
        let issued = GameAction::PlayLand {
            object_id: land,
            card_id: CardId(1),
        };
        assert!(contract.permits(&state, player, &issued));
        assert!(!contract.permits(
            &state,
            player,
            &GameAction::PlayLand {
                object_id: ObjectId(999),
                card_id: CardId(1),
            },
        ));
    }
}
