//! DRFTR is a utility library for creating Discord bots to draft anything with [Poise](https://docs.rs/poise/latest/poise/) and [Serenity](https://docs.rs/serenity/latest/serenity/).
//!
//! This library is designed to allow only one player to lock in their pick at a time, and for the draft pool to be a single shared pool.
//! In other words, it does not yet support things like Magic: the Gathering drafts, though that is a feature I intend to build.
#![allow(dead_code)]
mod draft_types;
use poise::serenity_prelude as serenity;
use std::collections::{HashMap, VecDeque};
type Draftable = Box<dyn DraftItem + 'static>;

/// A container for any number of draft [`League`]s in a single Discord server.
///
/// Each server your draft bot is in needs to have an associated DraftGuild.
/// Have your users run a /setup or /config command to set the default output channel and initialize an associated DraftGuild.
pub struct DraftGuild {
    id: u64,
    // k: name provided on League initialization
    leagues: HashMap<String, League>,
    default_output: serenity::ChannelId,
}

impl DraftGuild {
    /// Creates a new DraftGuild.
    ///
    /// The id parameter should be the Discord Guild ID which you can retrieve from poise::Context, but it is not wrapped
    /// in a serenity::GuildId enum so that you can set a custom *unique* identifier if you so choose.
    pub fn new(id: u64, default_output: serenity::ChannelId) -> DraftGuild {
        DraftGuild {
            id,
            leagues: HashMap::new(),
            default_output,
        }
    }
    /// Adds a [`League`] to the DraftGuild.
    ///
    /// Leagues are inserted into a HashMap whose keys are the Leagues' names.
    /// No more than one league with the same name can exist in a DraftGuild at any given time.
    pub fn add_league(
        &mut self,
        league: League,
    ) -> Result<&HashMap<String, League>, DraftGuildError> {
        if self.leagues.contains_key(&league.name) {
            return Err(DraftGuildError::LeagueNameAlreadyInUseError);
        }
        self.leagues.insert(league.name.clone(), league);
        Ok(&self.leagues)
    }
    /// Retrieves a [`League`] from the Draftguild's collection by name, if it exists.
    ///
    /// By forcing users to specify the name of the League they are sending commands for, you can allow users to participate in multiple drafts at once.
    pub fn league_by_name(&mut self, key: String) -> Result<&mut League, DraftGuildError> {
        if let Some(league) = self.leagues.get_mut(&key) {
            return Ok(league);
        }
        Err(DraftGuildError::LeagueNotFoundError)
    }
    /// Retrieves a [`League`] from the DraftGuild's collection by ID, if it exists.
    pub fn league_by_id(&mut self, id: u64) -> Result<&mut League, DraftGuildError> {
        if let Some(league) = self.leagues.values_mut().find(|league| league.id == id) {
            return Ok(league);
        }
        Err(DraftGuildError::LeagueNotFoundError)
    }
    /// Deletes a [`League`] by name, if it exists.
    pub fn delete_league(&mut self, key: String) -> Result<League, DraftGuildError> {
        if let Some(league) = self.leagues.remove(&key) {
            return Ok(league);
        }
        Err(DraftGuildError::LeagueNotFoundError)
    }
    /// Deletes a [`League`] by ID, if it exists.
    pub fn delete_league_by_id(&mut self, id: u64) -> Result<League, DraftGuildError> {
        if let Some(league) = self.leagues.values().find(|league| league.id == id) {
            let league_name = league.name.clone();
            let league = self.leagues.remove(&league_name).unwrap();
            return Ok(league);
        }
        Err(DraftGuildError::LeagueNotFoundError)
    }
    /// Deletes all leagues from the DraftGuild and returns a Vec of the deleted leagues.
    pub fn clear_leagues(&mut self) -> Vec<League> {
        let drained = self.leagues.drain();
        let mut leagues = Vec::new();
        for (_, v) in drained {
            leagues.push(v);
        }
        leagues
    }
}

#[derive(Debug)]
pub enum DraftGuildError {
    LeagueNotFoundError,
    LeagueNameAlreadyInUseError,
}

/// A specific ongoing draft league.
///
/// Recommend setting its ID to the interaction ID of the command that created it.
pub struct League {
    id: u64,
    // the player's index is their position in the draft
    players: Vec<ActivePlayer>,
    output: Option<serenity::ChannelId>,
    name: String,
    active: bool,
    current_seat: u32,
    total_picks: u32,
    draft_type: draft_types::DraftType,
    final_pick: u32,
}

impl League {
    /// Creates a new [`League`].
    ///
    /// Since users need to know the name of the league in order to refer to it in commands, set the name through a parameter in the command that creates it.
    ///
    /// team_size should be the number of [DraftItem]s on each team, e.g. 15 (11 starters + 4 substitutes) for fantasy football.
    ///
    /// To avoid cluttering a single channel with multiple draft announcements, output channels can be set for individual Leagues.
    /// Leagues with a None output should send their messages to the [`DraftGuild`]'s default_output.
    ///
    /// The current options for draft types are:
    ///
    /// * **Snake draft**:
    /// To snake draft, the pool of possible selections is passed from start to end with each player picking once. When the draft reaches the end of the table, the final player
    /// selects a second time, then the pool is passed back along the table in reverse order (the placement of starting settlements in Catan is snake draft).
    ///
    /// * **Linear draft**:
    /// A linear draft is more straightforward -- the pool of selections is passed around in a circle. Once the pool reaches the last player, that player passes it back to the first player.
    ///
    /// # Panics
    ///
    /// If the users Vec is empty, the program will panic.
    /// Draft organizers should have a method of populating this collection before initializing a new League - e.g. an "Add to Draft" context menu command.
    pub fn new(
        users: &[serenity::UserId],
        id: u64,
        name: String,
        output: Option<serenity::ChannelId>,
        draft_type: draft_types::DraftType,
        team_size: u32,
    ) -> League {
        let mut players = Vec::new();
        for id in users.iter() {
            players.push(ActivePlayer {
                picks: Vec::new(),
                queue: VecDeque::new(),
                id: *id,
            })
        }
        let final_pick = (players.len() as u32 * team_size) - 1;
        League {
            id,
            players,
            output,
            name,
            active: false,
            current_seat: 0,
            total_picks: 0,
            draft_type,
            final_pick,
        }
    }
    /// Moves the draft one seat forward and returns the [`ActivePlayer`] at that position, or
    /// None if the draft is complete.
    ///
    ///  If the draft is complete, the League is set to inactive.
    ///
    /// This method is used in [`League::lock`], and does not need to be implemented manually for the normal movement of the draft.
    /// However, it can be useful in a /skip command, where an absent player can be skipped to prevent a draft from stalling.
    /// Note that in this case, the draft will end with that player having selected one fewer Draftables than the other players -
    /// see [`League::add_to_player_picks`].
    pub fn advance(&mut self) -> Option<&mut ActivePlayer> {
        if self.total_picks == self.final_pick {
            self.deactivate();
            return None;
        }
        let next = match self.draft_type {
            draft_types::DraftType::Snake => {
                draft_types::snake_draft(self.total_picks, self.players.len() as u32)
            }
            draft_types::DraftType::Linear => {
                draft_types::linear_draft(self.total_picks, self.players.len() as u32)
            }
        };
        self.current_seat = next;
        self.total_picks += 1;
        Some(&mut self.players[next as usize])
    }
    /// Sets the League to active. An active League is one in which the draft portion of the competition is taking place,
    /// so waivers and trades are disabled.
    pub fn activate(&mut self) {
        self.active = true;
    }
    /// Sets the League to inactive. Inactive Leagues may stay in their DraftGuild's collection, but users cannot make picks while drafts are inactive.
    pub fn deactivate(&mut self) {
        self.active = false;
    }
    /// Returns the active status of the League.
    pub fn active(&self) -> bool {
        self.active
    }
    /// Records the pick argument, then recursively advances the draft, recording any picks that ActivePlayers have queued.
    ///
    /// Each time a pick is locked in, it is removed from each other ActivePlayer's queue.
    ///
    /// # Returns
    ///
    /// Returns a Vec of picks as tuples. The first element in the tuple is the UserId of the ActivePlayer who made the pick,
    /// and the second is the name() of the [`DraftItem`] (as a String, not a &str, for ownership reasons - gimme a break here I'm new to Rust).
    ///
    /// # Errors
    ///
    /// If the league is marked as inactive, returns a [`LeagueError::LeagueInactiveError`].
    pub fn lock(
        &mut self,
        pick: Draftable,
    ) -> Result<Vec<(serenity::UserId, String)>, LeagueError> {
        if !self.active {
            return Err(LeagueError::LeagueInactiveError);
        }
        Ok(self.lock_private(pick, Vec::new()))
    }
    fn lock_private(
        &mut self,
        pick: Draftable,
        returned_picks: Vec<(serenity::UserId, String)>,
    ) -> Vec<(serenity::UserId, String)> {
        let mut returned_picks = returned_picks;
        for player in &mut self.players {
            player.delete_from_queue(pick.name());
        }
        let current_player = &mut self.players[self.current_seat as usize];
        returned_picks.push((current_player.id, pick.name().to_string()));
        current_player.lock_in(pick);
        if let Some(next_player) = self.advance() {
            if let Some(pick) = next_player.first_in_queue() {
                returned_picks = self.lock_private(pick, returned_picks);
            }
        }
        returned_picks
    }
    /// Exchanges a player's [DraftItem] (waivered_from) for a [DraftItem] available in the pool (waivered_for).
    ///
    /// # Errors
    ///
    /// If the league is active, returns [`LeagueError::LeagueActiveError`].
    ///
    /// If waivered_for has been picked, it is not in the pool and must be traded for - returns [`LeagueError::DraftableInUseError`].
    ///
    /// If waivered_from is not in the player's list of picks, returns [`LeagueError::DraftableNotFoundError`].
    ///
    /// If the player is not in this league, returns [`LeagueError::PlayerNotFoundError`].
    pub fn waiver(
        &mut self,
        id: serenity::UserId,
        waivered_from: &str,
        waivered_for: Draftable,
    ) -> Result<&Vec<Draftable>, LeagueError> {
        if self.active {
            return Err(LeagueError::LeagueActiveError);
        };
        let all_picks = match self.all_picks() {
            Ok(picks) => picks,
            Err(_) => Vec::new(),
        };
        if all_picks.iter().any(|p| p.name() == waivered_for.name()) {
            return Err(LeagueError::DraftableInUseError);
        }
        if let Some(player) = self.get_player_mut(id) {
            if let Some(_) = player.delete_from_picks(waivered_from) {
                player.lock_in(waivered_for);
                return Ok(&player.picks);
            }
            return Err(LeagueError::DraftableNotFoundError);
        }
        Err(LeagueError::PlayerNotFoundError)
    }
    /// Trades item1 from user1 to user2 for item2.
    ///
    /// # Returns
    ///
    /// If Ok, returns a tuple of (user1's picks, user2's picks) updated with the trades.
    ///
    /// # Errors
    ///
    /// If the league is active, returns [`LeagueError::LeagueActiveError`].
    ///
    /// If user1 does not have item1, or user2 does not have item2, returns [`LeagueError::DraftableNotFoundError`].
    ///
    /// If either user1 or user2 are not in the draft, returns [`LeagueError::PlayerNotFoundError`].
    pub fn trade(
        &mut self,
        user1: serenity::UserId,
        item1: &str,
        user2: serenity::UserId,
        item2: &str,
    ) -> Result<(&Vec<Draftable>, &Vec<Draftable>), LeagueError> {
        if self.active {
            return Err(LeagueError::LeagueActiveError);
        };
        let Some(player1) = self.get_player_mut(user1) else {
            return Err(LeagueError::PlayerNotFoundError)
        };
        let Some(item1) = player1.delete_from_picks(item1) else {
            return Err(LeagueError::DraftableNotFoundError)
        };
        let Some(player2) = self.get_player_mut(user2) else {
            return Err(LeagueError::PlayerNotFoundError)
        };
        let Some(item2) = player2.delete_from_picks(item2) else {
            return Err(LeagueError::DraftableNotFoundError)
        };
        let p1 = self.get_player_mut(user1).unwrap();
        p1.lock_in(item2);
        let p2 = self.get_player_mut(user2).unwrap();
        p2.lock_in(item1);
        Ok((
            &self.get_player(user1).unwrap().picks,
            &self.get_player(user2).unwrap().picks,
        ))
    }
    /// Adds a Draftable to the given user's queue and returns the new queue.
    ///
    /// # Errors
    ///
    /// If there is no player in the league with the given ID, returns a [`LeagueError::PlayerNotFoundError`].
    pub fn add_to_player_queue(
        &mut self,
        id: serenity::UserId,
        item: Draftable,
    ) -> Result<&VecDeque<Draftable>, LeagueError> {
        if let Some(player) = self.get_player_mut(id) {
            player.add_to_queue(item);
            return Ok(&player.queue);
        }
        Err(LeagueError::PlayerNotFoundError)
    }
    /// Removes a Draftable from the player's queue and returns the removed item.
    ///
    /// # Errors
    ///
    /// If there is no player with the given ID, returns a [`LeagueError::PlayerNotFoundError`].
    /// If there is no Draftable with the given name in the player's queue, returns a [`LeagueError::DraftableNotFoundError`].
    pub fn delete_from_player_queue(
        &mut self,
        id: serenity::UserId,
        name: &str,
    ) -> Result<Draftable, LeagueError> {
        if let Some(player) = self.get_player_mut(id) {
            if let Some(item) = player.delete_from_queue(name) {
                return Ok(item);
            }
            return Err(LeagueError::DraftableNotFoundError);
        }
        Err(LeagueError::PlayerNotFoundError)
    }
    /// Returns a given player's queue.
    ///
    /// # Errors
    ///
    /// If there is no player with the given ID, returns a [`LeagueError::PlayerNotFoundError`].
    /// If the player's queue is empty, returns a [`LeagueError::PlayerQueueEmptyError`].
    pub fn player_queue(
        &mut self,
        id: serenity::UserId,
    ) -> Result<&VecDeque<Draftable>, LeagueError> {
        if let Some(player) = self.get_player(id) {
            if player.queue.is_empty() {
                return Err(LeagueError::PlayerQueueEmptyError);
            }
            return Ok(&player.queue);
        }
        Err(LeagueError::PlayerNotFoundError)
    }
    /// Returns a given player's locked in picks.
    ///
    /// # Errors
    ///
    /// If there is no player with the given ID, returns a [`LeagueError::PlayerNotFoundError`].
    /// If the player has not locked in any picks yet, returns a [`LeagueError::PlayerPicksEmptyError`].
    pub fn player_picks(&mut self, id: serenity::UserId) -> Result<&Vec<Draftable>, LeagueError> {
        if let Some(player) = self.get_player(id) {
            if player.picks.is_empty() {
                return Err(LeagueError::PlayerPicksEmptyError);
            }
            return Ok(&player.picks);
        }
        Err(LeagueError::PlayerNotFoundError)
    }
    /// Returns all picks made in the draft.
    ///
    /// # Errors
    ///
    /// If no one has made any picks, returns [`LeagueError::NoPicksError`].
    pub fn all_picks(&self) -> Result<Vec<&Draftable>, LeagueError> {
        let mut picks = Vec::new();
        for player in &self.players {
            for pick in &player.picks {
                picks.push(pick);
            }
        }
        if picks.is_empty() {
            return Err(LeagueError::NoPicksError);
        }
        Ok(picks)
    }
    /// Returns the player whose turn it is to pick.
    ///
    /// # Errors
    ///
    /// If the league is set as inactive, returns [`LeagueError::LeagueInactiveError`].
    pub fn current_player(&self) -> Result<&ActivePlayer, LeagueError> {
        if !self.active {
            return Err(LeagueError::LeagueInactiveError);
        }
        Ok(&self.players[self.current_seat as usize])
    }
    /// Empties a given player's queue, returning all deleted items.
    ///
    /// # Errors
    ///
    /// If there is no player with the given ID, returns a [`LeagueError::PlayerNotFoundError`].
    /// If the player's queue is empty before attempting to clear it, returns a [`LeagueError::PlayerQueueEmptyError`].
    pub fn clear_player_queue(
        &mut self,
        id: serenity::UserId,
    ) -> Result<Vec<Draftable>, LeagueError> {
        if let Some(player) = self.get_player_mut(id) {
            if player.queue.is_empty() {
                return Err(LeagueError::PlayerQueueEmptyError);
            }
            let drained = player.queue.drain(..);
            let mut cleared = Vec::new();
            for d in drained {
                cleared.push(d);
            }
            return Ok(cleared);
        }
        Err(LeagueError::PlayerNotFoundError)
    }
    /// Adds a Draftable directly to a player's list of picks, and returns that player's picks.
    ///
    /// Use sparingly - it is preferable to use [League::lock]. However, this has some use as part of an admin command allowing the draft
    /// organizer to pick Draftables for users who were skipped (see [League::advance]).
    ///
    /// # Errors
    ///
    /// If the given player is not in the draft, returns [`LeagueError::PlayerNotFoundError`].
    pub fn add_to_player_picks(
        &mut self,
        id: serenity::UserId,
        pick: Draftable,
    ) -> Result<&Vec<Draftable>, LeagueError> {
        let all_picks = match self.all_picks() {
            Ok(picks) => picks,
            Err(_) => Vec::new(),
        };
        if all_picks.iter().any(|p| p.name() == pick.name()) {
            return Err(LeagueError::DraftableInUseError);
        }
        if let Some(player) = self.get_player_mut(id) {
            player.lock_in(pick);
            return Ok(&player.picks);
        }
        Err(LeagueError::PlayerNotFoundError)
    }
    fn get_player_mut(&mut self, id: serenity::UserId) -> Option<&mut ActivePlayer> {
        self.players.iter_mut().find(|p| p.id.0 == id.0)
    }
    fn get_player(&self, id: serenity::UserId) -> Option<&ActivePlayer> {
        self.players.iter().find(|p| p.id == id)
    }
}

#[derive(Debug)]
pub enum LeagueError {
    PlayerNotFoundError,
    DraftableNotFoundError,
    DraftableInUseError,
    PlayerPicksEmptyError,
    PlayerQueueEmptyError,
    LeagueActiveError,
    LeagueInactiveError,
    NoPicksError,
}
/// A struct to represent a Discord user who is currently part of one or more Leagues.
///
/// All mutation of ActivePlayers can be handled through the [League] that owns them, and they are created automatically when initializing a [League].
pub struct ActivePlayer {
    picks: Vec<Draftable>,
    queue: VecDeque<Draftable>,
    id: serenity::UserId,
}

impl ActivePlayer {
    fn add_to_queue(&mut self, item: Draftable) {
        self.queue.push_back(item);
    }
    fn lock_in(&mut self, item: Draftable) {
        self.picks.push(item);
    }
    fn first_in_queue(&mut self) -> Option<Draftable> {
        self.queue.pop_front()
    }
    fn delete_from_queue(&mut self, name: &str) -> Option<Draftable> {
        let idx = self.queue.iter().position(|i| i.name() == name);
        if let Some(i) = idx {
            return self.queue.remove(i);
        }
        None
    }
    fn delete_from_picks(&mut self, item: &str) -> Option<Draftable> {
        if let Some(item) = self.picks.iter_mut().position(|i| i.name() == item) {
            return Some(self.picks.remove(item));
        }
        None
    }
}

/// Trait to implement on any type you make to represent the things being drafted.
pub trait DraftItem {
    /// Use this to expose the name, or any other *unique* identifier, for your DraftItem. Each DraftItem **must** return a *unique* name.
    fn name(&self) -> &str;
}

#[cfg(test)]
mod tests {

    use super::*;
    #[derive(Debug)]
    struct Pokemon {
        name: String,
    }
    impl DraftItem for Pokemon {
        fn name(&self) -> &str {
            self.name.as_str()
        }
    }

    #[test]
    fn trade_works() {
        let pikachu = Pokemon {
            name: "Pikachu".to_string(),
        };

        let boxed_pikachu = Box::new(pikachu);
        let mut p1 = ActivePlayer {
            id: serenity::UserId(69420),
            picks: Vec::new(),
            queue: VecDeque::new(),
        };
        p1.lock_in(boxed_pikachu);

        let eldegoss = Pokemon {
            name: "Eldegoss".to_string(),
        };

        let boxed_eldegoss = Box::new(eldegoss);
        let mut p2 = ActivePlayer {
            id: serenity::UserId(42069),
            picks: Vec::new(),
            queue: VecDeque::new(),
        };
        p2.lock_in(boxed_eldegoss);
        let mut league = League {
            id: 69420,
            players: Vec::from([p1, p2]),
            output: None,
            name: "Creenis".to_string(),
            active: false,
            current_seat: 0,
            total_picks: 3,
            draft_type: draft_types::DraftType::Snake,
            final_pick: 5,
        };
        let (p1picks, p2picks) = league
            .trade(
                serenity::UserId(69420),
                "Pikachu",
                serenity::UserId(42069),
                "Eldegoss",
            )
            .expect("this oughta work");
        assert_eq!(p1picks[0].name(), "Eldegoss");
        assert_eq!(p2picks[0].name(), "Pikachu");
    }
    #[test]
    #[should_panic]
    fn add_league_with_same_name_errors() {
        let mut guild = DraftGuild::new(69420, serenity::ChannelId(69420));
        let users = Vec::from([serenity::UserId(69420), serenity::UserId(42069)]);
        let league1 = League::new(
            &users,
            69420,
            "League1".to_string(),
            None,
            draft_types::DraftType::Snake,
            5,
        );
        let league2 = League::new(
            &users,
            69420,
            "League1".to_string(),
            None,
            draft_types::DraftType::Snake,
            5,
        );
        guild
            .add_league(league1)
            .expect("The first league with a given name should insert correctly");
        guild
            .add_league(league2)
            .expect("The second league with the same name should return an error and not insert.");
    }

    #[test]
    fn league_lock_picks_deletes_picked_items_from_queue_and_locks_available_picks() {
        let pikachu = Pokemon {
            name: "Pikachu".to_string(),
        };
        let quaxly = Pokemon {
            name: "Quaxly".to_string(),
        };
        let boxed_pikachu = Box::new(pikachu);
        let boxed_quaxly = Box::new(quaxly);
        let mut p1 = ActivePlayer {
            id: serenity::UserId(69420),
            picks: Vec::new(),
            queue: VecDeque::new(),
        };
        p1.add_to_queue(boxed_pikachu);
        p1.add_to_queue(boxed_quaxly);

        let pikachu = Pokemon {
            name: "Pikachu".to_string(),
        };
        let raichu = Pokemon {
            name: "Raichu".to_string(),
        };
        let boxed_pikachu = Box::new(pikachu);
        let boxed_raichu = Box::new(raichu);
        let mut p2 = ActivePlayer {
            id: serenity::UserId(42069),
            picks: Vec::new(),
            queue: VecDeque::new(),
        };
        p2.add_to_queue(boxed_pikachu);
        p2.add_to_queue(boxed_raichu);
        let mut league = League {
            id: 69420,
            players: Vec::from([p1, p2]),
            output: None,
            name: "Creenis".to_string(),
            active: true,
            current_seat: 0,
            total_picks: 3,
            draft_type: draft_types::DraftType::Snake,
            final_pick: 5,
        };
        league
            .lock(Box::new(Pokemon {
                name: "Pikachu".to_string(),
            }))
            .expect("this is fine");
        assert_eq!(league.players[0].picks[0].name(), "Pikachu");
        assert_eq!(league.players[0].picks[1].name(), "Quaxly");
        assert_eq!(league.players[1].picks[0].name(), "Raichu");
    }

    #[test]
    fn lock_picks_returns_correct_pick_data() {
        let pikachu = Pokemon {
            name: "Pikachu".to_string(),
        };
        let quaxly = Pokemon {
            name: "Quaxly".to_string(),
        };
        let boxed_pikachu = Box::new(pikachu);
        let boxed_quaxly = Box::new(quaxly);
        let mut p1 = ActivePlayer {
            id: serenity::UserId(69420),
            picks: Vec::new(),
            queue: VecDeque::new(),
        };
        p1.add_to_queue(boxed_pikachu);
        p1.add_to_queue(boxed_quaxly);

        let pikachu = Pokemon {
            name: "Pikachu".to_string(),
        };
        let raichu = Pokemon {
            name: "Raichu".to_string(),
        };
        let boxed_pikachu = Box::new(pikachu);
        let boxed_raichu = Box::new(raichu);
        let mut p2 = ActivePlayer {
            id: serenity::UserId(42069),
            picks: Vec::new(),
            queue: VecDeque::new(),
        };
        p2.add_to_queue(boxed_pikachu);
        p2.add_to_queue(boxed_raichu);
        let mut league = League {
            id: 69420,
            players: Vec::from([p1, p2]),
            output: None,
            name: "Creenis".to_string(),
            active: true,
            current_seat: 0,
            total_picks: 3,
            draft_type: draft_types::DraftType::Snake,
            final_pick: 5,
        };
        let picks = league
            .lock(Box::new(Pokemon {
                name: "Pikachu".to_string(),
            }))
            .expect("this is fine");
        let (u1, pokemon1) = &picks[0];
        let (u2, pokemon2) = &picks[1];
        let (u3, pokemon3) = &picks[2];
        assert_eq!(u1, u2);
        assert_ne!(u1, u3);
        assert_eq!(pokemon1.to_owned(), "Pikachu".to_string());
        assert_eq!(pokemon2.to_owned(), "Quaxly".to_string());
        assert_eq!(pokemon3.to_owned(), "Raichu".to_string());
    }

    #[test]
    #[should_panic]
    fn no_waivers_in_active_draft() {
        let pikachu = Pokemon {
            name: "Pikachu".to_string(),
        };
        let mut league = League {
            id: 69420,
            players: Vec::new(),
            output: None,
            name: "Cheenis".into(),
            active: true,
            current_seat: 0,
            total_picks: 0,
            draft_type: draft_types::DraftType::Snake,
            final_pick: 255,
        };
        league
            .waiver(serenity::UserId(69420), "pikachu", Box::new(pikachu))
            .expect("no waivers in active drafts");
    }
    #[test]
    fn draftable_in_use_error() {
        let pikachu = Pokemon {
            name: "Pikachu".to_string(),
        };
        let quaxly = Pokemon {
            name: "Quaxly".to_string(),
        };
        let boxed_pikachu = Box::new(pikachu);
        let boxed_quaxly = Box::new(quaxly);
        let mut p1 = ActivePlayer {
            id: serenity::UserId(69420),
            picks: Vec::new(),
            queue: VecDeque::new(),
        };
        p1.lock_in(boxed_pikachu);
        p1.lock_in(boxed_quaxly);
        let mut league = League {
            id: 69420,
            players: Vec::from([p1]),
            output: None,
            name: "Creenis".to_string(),
            active: false,
            current_seat: 0,
            total_picks: 3,
            draft_type: draft_types::DraftType::Snake,
            final_pick: 5,
        };
        let pikachu = Pokemon {
            name: "Pikachu".to_string(),
        };
        let boxed_pikachu = Box::new(pikachu);
        match league.waiver(serenity::UserId(69420), "Pikachu", boxed_pikachu) {
            Err(LeagueError::DraftableInUseError) => {}
            _ => panic!("wronge"),
        }
    }
    #[test]
    fn draftable_not_found_error() {
        let pikachu = Pokemon {
            name: "Pikachu".to_string(),
        };
        let quaxly = Pokemon {
            name: "Quaxly".to_string(),
        };
        let boxed_pikachu = Box::new(pikachu);
        let boxed_quaxly = Box::new(quaxly);
        let mut p1 = ActivePlayer {
            id: serenity::UserId(69420),
            picks: Vec::new(),
            queue: VecDeque::new(),
        };
        p1.lock_in(boxed_pikachu);
        p1.lock_in(boxed_quaxly);
        let mut league = League {
            id: 69420,
            players: Vec::from([p1]),
            output: None,
            name: "Creenis".to_string(),
            active: false,
            current_seat: 0,
            total_picks: 3,
            draft_type: draft_types::DraftType::Snake,
            final_pick: 5,
        };
        let amoonguss = Pokemon {
            name: "Amoonguss".to_string(),
        };
        let boxed_amoonguss = Box::new(amoonguss);
        match league.waiver(serenity::UserId(69420), "Raichu", boxed_amoonguss) {
            Err(LeagueError::DraftableNotFoundError) => {}
            _ => panic!("wronge"),
        }
    }
    #[test]
    #[should_panic]
    fn empty_league_hash_returns_none() {
        let mut guild = DraftGuild {
            id: 69420,
            leagues: HashMap::new(),
            default_output: serenity::ChannelId(69420),
        };
        guild
            .league_by_name("key".to_string())
            .expect("There's nothing in here!");
    }
    #[test]
    fn get_league_finds_correct_league() {
        let mut guild = DraftGuild {
            id: 69420,
            leagues: HashMap::new(),
            default_output: serenity::ChannelId(69420),
        };
        let users = Vec::from([serenity::UserId(69420), serenity::UserId(42069)]);
        let league = League::new(
            &users,
            69420,
            "Creenis".to_string(),
            None,
            draft_types::DraftType::Snake,
            3,
        );
        guild.add_league(league).expect("goodbye");
        let got_league = guild
            .league_by_name("Creenis".to_string())
            .expect("You had better not ever see this message");
        assert_eq!("Creenis".to_string(), got_league.name);
    }

    #[test]
    fn returns_next_player() {
        let users = Vec::from([serenity::UserId(69420), serenity::UserId(42069)]);
        let mut league = League::new(
            &users,
            69420,
            "Creenis".to_string(),
            None,
            draft_types::DraftType::Snake,
            3,
        );
        let player = league.advance().unwrap();
        assert_eq!(player.id.0, 42069);
        assert_eq!(league.players.len(), 2);
    }

    #[test]
    #[should_panic]
    fn final_pick_returns_none() {
        let users = Vec::from([serenity::UserId(69420), serenity::UserId(42069)]);
        let mut league = League::new(
            &users,
            69420,
            "Creenis".to_string(),
            None,
            draft_types::DraftType::Snake,
            1,
        );
        let _player1 = league.advance();
        let _player2 = league.advance().unwrap();
    }
    #[test]
    fn delete_from_queue_deletes() {
        let pikachu = Pokemon {
            name: "Pikachu".to_string(),
        };
        let mut player = ActivePlayer {
            picks: Vec::new(),
            queue: VecDeque::new(),
            id: serenity::UserId(69420),
        };
        player.add_to_queue(Box::new(pikachu));
        assert_eq!(player.queue.len(), 1);
        let removed = player.delete_from_queue("Pikachu").unwrap();
        let removed = removed.name();
        assert_eq!(removed, "Pikachu");
        assert_eq!(player.queue.len(), 0);
    }

    #[test]
    #[should_panic]
    fn try_delete_item_not_in_queue() {
        let mut player = ActivePlayer {
            picks: Vec::new(),
            queue: VecDeque::new(),
            id: serenity::UserId(69420),
        };
        let _removed = player.delete_from_queue("Pikachu").unwrap();
    }

    #[test]
    fn gets_first_in_queue() {
        let pikachu = Pokemon {
            name: "Pikachu".to_string(),
        };
        let quaxly = Pokemon {
            name: "Quaxly".to_string(),
        };
        let mut player = ActivePlayer {
            picks: Vec::new(),
            queue: VecDeque::new(),
            id: serenity::UserId(69420),
        };
        player.add_to_queue(Box::new(pikachu));
        player.add_to_queue(Box::new(quaxly));
        let pikachu = player.first_in_queue().unwrap();
        assert_eq!(pikachu.name(), "Pikachu");
    }
}
