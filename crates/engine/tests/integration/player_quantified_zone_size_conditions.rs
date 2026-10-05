//! Player-quantified zone-size conditions — "a library has twenty or fewer
//! cards in it" (Shelldock Isle, Isleback Spawn) and "each player has no cards
//! in hand" (Howltooth Hollow).
//!
//! - CR 702.75a + CR 608.2c: a Hideaway land's "You may play the exiled card
//!   without paying its mana cost if <condition>" checks the condition as the
//!   ability resolves; when it is false nothing is offered.
//! - CR 608.2h: the census reads the game once, when the effect is applied.
//! - CR 400.1: each player has their own library and hand. Shelldock Isle's
//!   ruling: "It doesn't matter which library has twenty or fewer cards in it,
//!   and you don't have to specify a library." — opponents' libraries count.
//! - CR 611.3a: Isleback Spawn's static is re-evaluated continuously, so it
//!   switches on as soon as a library drops to twenty.
//!
//! All boards stay on P0's first main phase: the land's "enters tapped" is
//! undone by a free "Untap target permanent." spell, so library counts are
//! exact (no draw steps): P0's library is its seeded count − 1 after Hideaway
//! exiles one of the top four, and P1's library is exactly as seeded.

use engine::game::scenario::{GameRunner, GameScenario, P0, P1};
use engine::types::ability::CastingPermission;
use engine::types::actions::GameAction;
use engine::types::game_state::{ExileLinkKind, WaitingFor};
use engine::types::identifiers::ObjectId;
use engine::types::mana::{ManaCost, ManaType, ManaUnit};
use engine::types::phase::Phase;
use engine::types::player::PlayerId;
use engine::types::zones::Zone;

const SHELLDOCK_ISLE: &str = "Hideaway 4 (When this land enters, look at the top four cards of your library, exile one face down, then put the rest on the bottom in a random order.)\nThis land enters tapped.\n{T}: Add {U}.\n{U}, {T}: You may play the exiled card without paying its mana cost if a library has twenty or fewer cards in it.";
const HOWLTOOTH_HOLLOW: &str = "Hideaway 4 (When this land enters, look at the top four cards of your library, exile one face down, then put the rest on the bottom in a random order.)\nThis land enters tapped.\n{T}: Add {B}.\n{B}, {T}: You may play the exiled card without paying its mana cost if each player has no cards in hand.";
const ISLEBACK_SPAWN: &str = "Shroud (This creature can't be the target of spells or abilities.)\nThis creature gets +4/+8 as long as a library has twenty or fewer cards in it.";
const UNTAP_PERMANENT: &str = "Untap target permanent.";
const MILL_ONE: &str = "Target player mills a card.";

/// A Hideaway land on P0's battlefield with a hidden card, untapped and ready
/// to activate its play ability.
struct HideawayBoard {
    runner: GameRunner,
    land: ObjectId,
    hidden: ObjectId,
}

fn seed_library(scenario: &mut GameScenario, player: PlayerId, count: usize) {
    for i in 0..count {
        scenario.add_card_to_library_top(player, &format!("Library Card {i}"));
    }
}

fn free_spell(scenario: &mut GameScenario, player: PlayerId, name: &str, text: &str) -> ObjectId {
    scenario
        .add_spell_to_hand_from_oracle(player, name, true, text)
        .with_mana_cost(ManaCost::zero())
        .id()
}

/// Drive the Hideaway trigger, hiding the first offered card.
fn drive_hideaway(runner: &mut GameRunner) -> ObjectId {
    let mut chosen = None;
    for _ in 0..80 {
        match runner.state().waiting_for.clone() {
            WaitingFor::DigChoice { cards, .. } => {
                chosen = Some(cards[0]);
                runner
                    .act(GameAction::SelectCards {
                        cards: vec![cards[0]],
                    })
                    .expect("hiding a card");
            }
            WaitingFor::Priority { .. } if runner.state().stack.is_empty() && chosen.is_some() => {
                break
            }
            WaitingFor::Priority { .. } => {
                runner
                    .act(GameAction::PassPriority)
                    .expect("passing priority");
            }
            other => panic!("unexpected prompt while hiding a card: {other:?}"),
        }
    }
    chosen.expect("the Hideaway trigger offered a DigChoice")
}

/// Build the board: P0 plays the land (`text`), hides a card, and untaps the
/// land with a free spell. `setup` runs on the scenario before it is built.
fn hideaway_board(
    name: &str,
    text: &str,
    p0_library: usize,
    p1_library: usize,
    setup: impl FnOnce(&mut GameScenario),
) -> HideawayBoard {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);
    let land = scenario
        .add_land_to_hand(P0, name)
        .from_oracle_text(text)
        .id();
    seed_library(&mut scenario, P0, p0_library);
    seed_library(&mut scenario, P1, p1_library);
    let untap = free_spell(&mut scenario, P0, "Untap Spell", UNTAP_PERMANENT);
    setup(&mut scenario);
    let mut runner = scenario.build();

    let card_id = runner.state().objects[&land].card_id;
    runner
        .act(GameAction::PlayLand {
            object_id: land,
            card_id,
        })
        .expect("playing the Hideaway land");
    let hidden = drive_hideaway(&mut runner);
    assert!(
        runner.state().objects[&land].tapped,
        "reach guard: the land enters tapped"
    );
    runner.cast(untap).target_objects(&[land]).resolve();
    assert!(
        !runner.state().objects[&land].tapped,
        "reach guard: the free untap spell untapped the land"
    );
    HideawayBoard {
        runner,
        land,
        hidden,
    }
}

fn add_mana(runner: &mut GameRunner, mana: ManaType) {
    runner.state_mut().players[0]
        .mana_pool
        .add(ManaUnit::new(mana, ObjectId(0), false, vec![]));
}

/// Activate the land's play ability (index 1), accept any offer, and settle
/// the stack. Returns whether an `OptionalEffectChoice` was offered.
fn activate_play_ability(board: &mut HideawayBoard, mana: ManaType) -> bool {
    add_mana(&mut board.runner, mana);
    board
        .runner
        .act(GameAction::ActivateAbility {
            source_id: board.land,
            ability_index: 1,
        })
        .expect("activating the play ability must be legal");
    let mut offered = false;
    for _ in 0..40 {
        match board.runner.state().waiting_for.clone() {
            WaitingFor::OptionalEffectChoice { .. } => {
                offered = true;
                board
                    .runner
                    .act(GameAction::DecideOptionalEffect { accept: true })
                    .expect("accepting the offer");
            }
            WaitingFor::Priority { .. } if board.runner.state().stack.is_empty() => break,
            WaitingFor::Priority { .. } => {
                board
                    .runner
                    .act(GameAction::PassPriority)
                    .expect("passing priority");
            }
            other => panic!("unexpected prompt while activating: {other:?}"),
        }
    }
    offered
}

fn hidden_may_be_played(board: &HideawayBoard) -> bool {
    board.runner.state().objects[&board.hidden]
        .casting_permissions
        .iter()
        .any(|p| {
            matches!(
                p,
                CastingPermission::PlayFromExile {
                    source_id: Some(src),
                    ..
                } if *src == board.land
            )
        })
}

fn hidden_has_free_cast(board: &HideawayBoard) -> bool {
    board.runner.state().objects[&board.hidden]
        .casting_permissions
        .iter()
        .any(|p| matches!(p, CastingPermission::ExileWithAltCost { .. }))
}

/// Reach guards for the "nothing offered" cases: the activation resolved
/// (land tapped, stack empty) and the hidden card is still face-down exiled
/// and linked to the land.
fn assert_resolved_with_hidden_card_untouched(board: &HideawayBoard) {
    let state = board.runner.state();
    assert!(
        state.objects[&board.land].tapped,
        "reach guard: the activation paid its {{T}} cost"
    );
    assert!(state.stack.is_empty(), "reach guard: the ability resolved");
    let hidden = &state.objects[&board.hidden];
    assert_eq!(hidden.zone, Zone::Exile);
    assert!(hidden.face_down);
    assert!(state.exile_links.iter().any(|link| {
        link.exiled_id == board.hidden
            && link.source_id == board.land
            && matches!(link.kind, ExileLinkKind::HideawayLookable { .. })
    }));
}

fn library_len(runner: &GameRunner, player: PlayerId) -> usize {
    runner
        .state()
        .players
        .iter()
        .find(|p| p.id == player)
        .expect("player exists")
        .library
        .len()
}

/// CR 400.1 + CR 608.2h: only the OPPONENT's library is at twenty — the
/// existential "a library" still holds, so the play is offered.
#[test]
fn shelldock_isle_offers_play_when_an_opponents_library_has_twenty_cards() {
    let mut board = hideaway_board("Shelldock Isle", SHELLDOCK_ISLE, 31, 20, |_| {});
    assert_eq!(library_len(&board.runner, P0), 30);
    assert_eq!(library_len(&board.runner, P1), 20);
    assert!(
        activate_play_ability(&mut board, ManaType::Blue),
        "P1's 20-card library satisfies the gate"
    );
    assert!(hidden_may_be_played(&board));
}

/// CR 608.2c + CR 702.75a: every library holds more than twenty — the gate is
/// false on resolution, so no play is offered or granted.
#[test]
fn shelldock_isle_offers_nothing_when_every_library_has_more_than_twenty_cards() {
    let mut board = hideaway_board("Shelldock Isle", SHELLDOCK_ISLE, 31, 21, |_| {});
    assert_eq!(library_len(&board.runner, P0), 30);
    assert_eq!(library_len(&board.runner, P1), 21);
    assert!(
        !activate_play_ability(&mut board, ManaType::Blue),
        "no library has twenty or fewer cards"
    );
    assert_resolved_with_hidden_card_untouched(&board);
    assert!(!hidden_may_be_played(&board));
    assert!(!hidden_has_free_cast(&board));
}

/// CR 102.1 + CR 402.1: every hand (P0's emptied by playing the land and
/// casting the untap spell) is empty, so Howltooth Hollow offers the play.
#[test]
fn howltooth_hollow_offers_play_when_every_hand_is_empty() {
    let mut board = hideaway_board("Howltooth Hollow", HOWLTOOTH_HOLLOW, 10, 10, |_| {});
    assert!(board.runner.state().players[0].hand.is_empty());
    assert!(board.runner.state().players[1].hand.is_empty());
    assert!(
        activate_play_ability(&mut board, ManaType::Black),
        "every hand is empty"
    );
    assert!(hidden_may_be_played(&board));
}

/// An opponent holding a card falsifies "each player has no cards in hand".
#[test]
fn howltooth_hollow_offers_nothing_when_an_opponent_holds_a_card() {
    let mut board = hideaway_board("Howltooth Hollow", HOWLTOOTH_HOLLOW, 10, 10, |s| {
        s.add_card_to_hand(P1, "Held Card");
    });
    assert!(board.runner.state().players[0].hand.is_empty());
    assert_eq!(board.runner.state().players[1].hand.len(), 1);
    assert!(
        !activate_play_ability(&mut board, ManaType::Black),
        "P1 holds a card"
    );
    assert_resolved_with_hidden_card_untouched(&board);
    assert!(!hidden_may_be_played(&board));
    assert!(!hidden_has_free_cast(&board));
}

/// CR 102.1: "each player" includes the controller — an "each opponent"
/// misreading would wrongly offer the play here.
#[test]
fn howltooth_hollow_offers_nothing_when_its_controller_holds_a_card() {
    let mut board = hideaway_board("Howltooth Hollow", HOWLTOOTH_HOLLOW, 10, 10, |s| {
        s.add_card_to_hand(P0, "Held Card");
    });
    assert_eq!(board.runner.state().players[0].hand.len(), 1);
    assert!(board.runner.state().players[1].hand.is_empty());
    assert!(
        !activate_play_ability(&mut board, ManaType::Black),
        "the controller holds a card"
    );
    assert_resolved_with_hidden_card_untouched(&board);
    assert!(!hidden_may_be_played(&board));
}

/// CR 611.3a: Isleback Spawn's +4/+8 applies as soon as any library (here the
/// opponent's) drops to twenty.
#[test]
fn isleback_spawn_gets_plus_four_plus_eight_once_a_library_drops_to_twenty() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);
    let spawn = scenario
        .add_creature(P0, "Isleback Spawn", 4, 8)
        .from_oracle_text_with_keywords(&["Shroud"], ISLEBACK_SPAWN)
        .id();
    // P1's library starts at 22 so the second mill crosses the twenty-card
    // threshold; the first mill also forces a layer pass for the 4/8 baseline.
    seed_library(&mut scenario, P0, 30);
    seed_library(&mut scenario, P1, 22);
    let first_mill = free_spell(&mut scenario, P0, "Mill One", MILL_ONE);
    let second_mill = free_spell(&mut scenario, P0, "Mill Again", MILL_ONE);
    let mut runner = scenario.build();

    runner.cast(first_mill).target_player(P1).resolve();
    assert_eq!(library_len(&runner, P1), 21, "reach guard: P1 milled one");
    let obj = &runner.state().objects[&spawn];
    assert_eq!(
        (obj.power, obj.toughness),
        (Some(4), Some(8)),
        "every library still holds more than twenty cards"
    );

    runner.cast(second_mill).target_player(P1).resolve();
    assert_eq!(library_len(&runner, P1), 20, "reach guard: P1 milled again");
    let obj = &runner.state().objects[&spawn];
    assert_eq!(
        (obj.power, obj.toughness),
        (Some(8), Some(16)),
        "P1's library is at twenty — the static applies"
    );
}
