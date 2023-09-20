# DRFTR

This is a utility library designed to integrate into Discord bots built with [Poise](https://crates.io/crates/poise) and [Serenity](https://crates.io/crates/serenity). You can use the structs and methods found within to build a draft league manager bot.

I built this library primarily for personal use, but I believe it can be helpful for others. See the [full documentation](https://docs.rs/drftr/latest/drftr/) for more detailed information.

# Setup

After adding this library to your Rust Discord bot, make sure to create a struct which implements the DraftItem trait. This struct _must_ have a method which returns a unique name, but otherwise can contain as little or as much data as you choose. You may want each DraftItem to contain a URL to an image of the item, for example, or a set of relevant statistics (points, KDA, position, etc).

Your bot should also contain a static collection that represents the pool of possible draft items. This collection needs not contain actual DraftItems, but you should have a constructor to create DraftItems from entries within it.

When passing these DraftItems into DRFTR methods, you must also put them into a Box, and they will be Boxed when a method returns a DraftItem.

# Workflow

The general workflow of the structures in this library is intended as such:

-   1. An end user adds your bot to their Discord server, and calls a command such as /configure or /setup. This command should execute the DraftGuild::new associated function, and the returned DraftGuild should be stored in a collection within your bot.

-   2. The user calls a command to create a new League -- a specific draft. This should execute the League::new associated function, then add that League to the DraftGuild the command originated in with the DraftGuild::add*league method. This does \_not* activate the League -- in my experience, it is generally preferred to separate creating and starting the League, so that a draft organizer can do the necessary configuration first, then start the league when all players are ready.

-   3. The user calls a command to _begin_ the League. This should execute the League::activate method, then send a message to the output channel either specified in the League, or the one chosen as the default output channel for the DraftGuild. This message should alert the first player that it is their turn to pick in the draft.

-   4. Players call a command to lock in their picks. If the player calling is the active seat in the draft, then the League::lock method should be executed, and otherwise the League::add_to_player_queue method. Note that players' queues do not have to be locked in manually in your bot -- the League::lock method will visit each player's queue in order and lock in the first pick they have queued when it is called. Remember to send announcements informing each player when it is their turn to pick! The ActivePlayer struct contains an id field which should be set to the player's Discord ID, so that user mentions are trivial.

Players should also have a command to remove picks from their queues, which should call League::delete_from_player_queue, and a command to clear their queues, which should call League::clear_player_queue.

-   5. When the draft has run its course, the bot should send a message announcing that. The draft will then automatically be set as inactive, so players will no longer be able to lock in picks. They will still be able to call the pick command, but DRFTR will return a LeagueError::LeagueInactiveError, which can be transformed to an error reply in your bot.

-   6. Players call commands to waiver or trade their picks during the season - these should call League::waiver and League::trade, respectively.

DraftGuilds can contain any number of Leagues, whether active or inactive, but each Discord server should be associated with only one DraftGuild. You are responsible for maintaining the collection of DraftGuilds in your bot.
