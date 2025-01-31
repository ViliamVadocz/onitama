use std::fmt::Debug;

use bitintr::{Andn, Popcnt};
use nudge::assume;

use crate::ops::{BitIter, CardIter};
use crate::{SHIFTED, SHIFTED_L, SHIFTED_R, SHIFTED_U};

pub const PIECE_MASK: u32 = (1 << 25) - 1;

#[derive(Clone, Copy, PartialEq, Hash, Default)]
pub struct Game {
    pub my: u32,
    pub other: u32,
    pub cards: u32,
    pub table: u32,
}

impl Debug for Game {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!(
            "\nx: {}, o: {}",
            self.my.wrapping_shr(25),
            self.other.wrapping_shr(25)
        ))?;
        for i in 0..5 {
            f.write_str("\n")?;
            for j in 0..5 {
                let pos = i * 5 + j;
                if self.my & 1 << pos != 0 {
                    f.write_str("x")?;
                } else if self.other & 1 << 24 >> pos != 0 {
                    f.write_str("o")?;
                } else {
                    f.write_str(".")?;
                }
            }
        }
        Ok(())
    }
}

impl Game {
    #[inline(always)]
    pub fn count_moves(&self) -> u64 {
        let mut total = 0;
        for from in self.next_my() {
            let both = unsafe {
                let mut cards = self.next_my_card();
                SHIFTED_L
                    .get_unchecked(cards.next().unwrap() as usize)
                    .get_unchecked(from as usize)
                    | SHIFTED_U
                        .get_unchecked(cards.next().unwrap() as usize)
                        .get_unchecked(from as usize)
            };
            let my = self.my as u64 | (self.my as u64) << 32;
            total += my.andn(both).popcnt();
        }
        total
    }

    #[inline]
    pub fn is_win(&self) -> bool {
        for from in self.next_my() {
            let both = unsafe {
                let mut cards = self.next_my_card();
                SHIFTED
                    .get_unchecked(cards.next().unwrap() as usize)
                    .get_unchecked(from as usize)
                    | SHIFTED
                        .get_unchecked(cards.next().unwrap() as usize)
                        .get_unchecked(from as usize)
            };
            let other_king = 1 << 24 >> self.other.wrapping_shr(25);
            if both & other_king != 0 {
                return true;
            }
            if from == self.my.wrapping_shr(25) && both & (1 << 22) != 0 {
                return true;
            }
        }
        false
    }

    #[inline]
    pub fn count_pieces(&self) -> usize {
        (self.my & PIECE_MASK).popcnt() as usize
    }

    #[inline]
    pub fn is_loss(&self) -> bool {
        self.other.wrapping_shr(25) == 22 || self.my & 1 << self.my.wrapping_shr(25) == 0
    }

    #[inline]
    pub fn is_other_loss(&self) -> bool {
        self.my.wrapping_shr(25) == 22 || self.other & 1 << self.other.wrapping_shr(25) == 0
    }

    #[inline]
    fn next_my(&self) -> BitIter {
        unsafe { assume(self.my & PIECE_MASK != 0) }
        BitIter(self.my & PIECE_MASK)
    }

    #[inline]
    pub fn next_other(&self) -> BitIter {
        unsafe { assume(self.other & PIECE_MASK != 0) }
        BitIter(self.other & PIECE_MASK)
    }

    #[inline]
    fn next_my_card(&self) -> CardIter {
        CardIter::new(self.cards)
    }

    #[inline]
    fn next_other_card(&self) -> CardIter {
        CardIter::new(self.cards.wrapping_shr(16))
    }

    #[inline]
    fn next_to(&self, from: u32, card: u32) -> BitIter {
        let &shifted = unsafe {
            SHIFTED
                .get_unchecked(card as usize)
                .get_unchecked(from as usize)
        };
        BitIter(self.my.andn(shifted))
    }

    #[inline]
    fn next_from(&self, to: u32, card: u32) -> BitIter {
        let &shifted = unsafe {
            SHIFTED_R
                .get_unchecked(card as usize)
                .get_unchecked(to as usize)
        };
        let mut my_rev = self.my.reverse_bits() >> 7;
        if to == self.other.wrapping_shr(25) {
            my_rev |= 1 << 22
        }
        BitIter(my_rev.andn(self.other.andn(shifted)))
    }

    #[inline]
    pub fn forward(&self) -> GameIter {
        let mut from = self.next_my();
        let from_curr = from.next().unwrap();
        let mut card = self.next_my_card();
        let card_curr = card.next().unwrap();
        let to = self.next_to(from_curr, card_curr);
        GameIter {
            game: self,
            from,
            from_curr,
            card,
            card_curr,
            to,
        }
    }

    #[inline]
    pub fn backward(&self) -> GameBackIter {
        let mut to = self.next_other();
        let to_curr = to.next().unwrap();
        let mut card = self.next_other_card();
        let card_curr = card.next().unwrap();
        let from = self.next_from(to_curr, self.table);
        GameBackIter {
            game: self,
            to,
            to_curr,
            card,
            card_curr,
            from,
        }
    }
}

pub struct GameIter<'a> {
    game: &'a Game,
    from: BitIter,
    from_curr: u32,
    card: CardIter,
    card_curr: u32,
    to: BitIter,
}

impl Iterator for GameIter<'_> {
    type Item = Game;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let mut to_new = self.to.next();
        while to_new.is_none() {
            let mut card_new = self.card.next();
            if card_new.is_none() {
                self.from_curr = self.from.next()?;
                self.card = self.game.next_my_card();
                card_new = self.card.next();
            }
            self.card_curr = card_new.unwrap();
            self.to = self.game.next_to(self.from_curr, self.card_curr);
            to_new = self.to.next();
        }
        let to_curr = to_new.unwrap();

        let my_king = self.game.my.wrapping_shr(25);

        let to_other = 1 << 24 >> to_curr;
        let other = to_other.andn(self.game.other);

        let my_cards = self.game.cards ^ 1 << self.card_curr ^ 1 << self.game.table;
        let cards = my_cards.wrapping_shl(16) | my_cards.wrapping_shr(16);

        let mut my = self.game.my ^ (1 << self.from_curr) ^ (1 << to_curr);

        if self.from_curr == my_king {
            my = my & PIECE_MASK | to_curr << 25;
        };

        let new_game = Game {
            other: my,
            my: other,
            cards,
            table: self.card_curr,
        };
        Some(new_game)
    }
}

impl ExactSizeIterator for GameIter<'_> {
    fn len(&self) -> usize {
        self.game.count_moves() as usize
    }
}

pub struct GameBackIter<'a> {
    game: &'a Game,
    to: BitIter,
    to_curr: u32,
    card: CardIter,
    card_curr: u32,
    from: BitIter,
}

impl Iterator for GameBackIter<'_> {
    type Item = (Game, u32); // (no)take

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let mut from_new = self.from.next();
        while from_new.is_none() {
            let mut card_new = self.card.next();
            if card_new.is_none() {
                self.to_curr = self.to.next()?;
                self.card = self.game.next_other_card();
                card_new = self.card.next();
            }
            self.card_curr = card_new.unwrap();
            self.from = self.game.next_from(self.to_curr, self.game.table);
            from_new = self.from.next();
        }
        let from_curr = from_new.unwrap();

        let other_king = self.game.other.wrapping_shr(25);

        let cards = self.game.cards.wrapping_shl(16) | self.game.cards.wrapping_shr(16);
        let cards = cards ^ 1 << self.card_curr ^ 1 << self.game.table;
        let mut other = self.game.other ^ (1 << self.to_curr) ^ (1 << from_curr);

        if self.to_curr == other_king {
            other = other & PIECE_MASK | from_curr << 25;
        };

        let prev_game = Game {
            my: other,
            other: self.game.my,
            cards,
            table: self.card_curr,
        };
        Some((prev_game, (1 << 24) >> self.to_curr))
    }
}
