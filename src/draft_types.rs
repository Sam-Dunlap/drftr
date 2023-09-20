pub enum DraftType {
    Snake,
    Linear,
}

pub fn snake_draft(total_picks: u32, number_of_drafters: u32) -> u32 {
    let mut next_seat = 0;

    for i in 0..=(total_picks + 1) {
        if i % number_of_drafters == 0 {
            continue;
        };
        if i % (2 * number_of_drafters) <= number_of_drafters {
            next_seat += 1;
        } else {
            next_seat -= 1;
        }
    }
    next_seat
}

/// Returns the *next* seat in the draft
pub fn linear_draft(total_picks: u32, number_of_drafters: u32) -> u32 {
    (total_picks + 1) % number_of_drafters
}

#[cfg(test)]
mod draft_type_tests {
    use super::*;

    #[test]
    fn snake_draft_returns_correct_next_seat() {
        assert_eq!(snake_draft(7, 5), 1);
    }

    #[test]
    fn linear_draft_returns_correct_next_seat() {
        assert_eq!(linear_draft(4, 5), 0);
        assert_eq!(linear_draft(5, 5), 1);
    }
}
