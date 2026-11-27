//! A pack's player-only weapon, offered only when its player is picked, in a process of its own: installing reads
//! the game's data once.

use last_harvest_game::data::{self, Card, Data};
use last_harvest_game::{Tier, player_looks, player_powerups};

fn card(name: &str, text: &str) -> Card {
    Card {
        detail: 1,
        worn: String::new(),
        name: name.into(),
        text: text.into(),
        frame: [16, 16],
        sheet: vec![255; 16 * 3 * 16 * 5 * 4],
    }
}

#[test]
fn a_player_only_weapon_goes_only_to_its_player_and_the_rest_stay_global() {
    let own = last_harvest_game::built_in_table("powerups.txt").unwrap();
    let powerups = format!(
        "{own}\n[urumi]\nname Urumi\nkind weapon\nicon WHITE\ndoes blade\nplayer kd\n\n[katta]\nname Desi Katta\nkind weapon\nicon WHITE\ndoes pistol\n\n[lost]\nkind weapon\nicon WHITE\ndoes knife\nplayer nobody\n"
    );
    let players = vec![
        card(
            "kd.player.png",
            "[kd]\nname Kuldeep\nstart normal urumi knife\n",
        ),
        card("jo.player.png", "[jo]\nstart normal urumi pistol\n"),
    ];
    let notes = data::install(Data {
        powerups: Some(powerups),
        players,
        ..Data::default()
    })
    .unwrap();
    assert_eq!(
        notes,
        ["powerups.txt [lost]: no player card is [nobody], so no one is offered it"]
    );

    let has = |id: &str, up: &str| player_powerups(id).iter().any(|p| p == up);
    assert!(
        has("kd", "urumi") && !has("jo", "urumi") && !has("", "urumi"),
        "only Kuldeep is offered the Urumi"
    );
    assert!(
        has("kd", "katta") && has("jo", "katta") && has("", "katta"),
        "a weapon without a player line is everyone's"
    );
    assert!(has("jo", "pistol") && !has("kd", "lost"));

    let look = |id: &str| player_looks().iter().find(|l| l.id == id).unwrap();
    assert_eq!(look("kd").start_names(Tier::Normal), ["Urumi", "Knife"]);
    assert_eq!(
        look("jo").start_names(Tier::Normal),
        ["Pistol"],
        "another player's start passes over it"
    );
}
