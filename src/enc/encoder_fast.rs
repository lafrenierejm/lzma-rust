use super::{
    encoder::LZMAEncoderTrait,
    lz::{LZEncoder, MFType},
    MATCH_LEN_MAX, MATCH_LEN_MIN, REPS,
};

#[derive(Default)]
pub struct FashEncoderMode {}

impl FashEncoderMode {
    pub const EXTRA_SIZE_BEFORE: u64 = 1;
    pub const EXTRA_SIZE_AFTER: u64 = MATCH_LEN_MAX as u64 - 1;

    pub fn get_memory_usage(dict_size: u64, extra_size_before: u64, mf: MFType) -> u64 {
        LZEncoder::get_memory_usage(
            dict_size,
            extra_size_before.max(Self::EXTRA_SIZE_BEFORE),
            Self::EXTRA_SIZE_AFTER,
            MATCH_LEN_MAX as u64,
            mf,
        )
    }
}
fn change_pair(small_dist: u64, big_dist: u64) -> bool {
    small_dist < (big_dist >> 7)
}
impl LZMAEncoderTrait for FashEncoderMode {
    fn get_next_symbol(&mut self, encoder: &mut super::encoder::LZMAEncoder) -> u64 {
        if encoder.data.read_ahead == -1 {
            encoder.find_matches();
        }

        encoder.data.back = -1;
        let avail = encoder.lz.data.get_avail().min(MATCH_LEN_MAX as i64);
        if avail < MATCH_LEN_MIN as i64 {
            return 1;
        }
        let mut best_rep_len = 0;
        let mut best_rep_index = 0;
        for rep in 0..REPS {
            let len = encoder.lz.data.get_match_len(encoder.reps[rep], avail);
            if len < MATCH_LEN_MIN {
                continue;
            }
            if len >= encoder.data.nice_len {
                encoder.data.back = rep as i64;
                encoder.skip(len - 1);
                return len as u64;
            }
            if len > best_rep_len {
                best_rep_index = rep;
                best_rep_len = len;
            }
        }

        let mut main_len = 0;
        let mut main_dist = 0;
        let matches = encoder.lz.matches();
        if matches.count > 0 {
            main_len = matches.len[matches.count as usize - 1];
            main_dist = matches.dist[matches.count as usize - 1];

            if main_len >= encoder.data.nice_len as u64 {
                encoder.data.back = (main_dist + REPS as i64) as _;
                encoder.skip((main_len - 1) as _);
                return main_len;
            }

            while matches.count > 1 && main_len == matches.len[matches.count as usize - 2] + 1 {
                if !change_pair(
                    matches.dist[matches.count as usize - 2] as u64,
                    main_dist as u64,
                ) {
                    break;
                }
                matches.count -= 1;
                main_len = matches.len[matches.count as usize - 1];
                main_dist = matches.dist[matches.count as usize - 1];
            }

            if main_len == MATCH_LEN_MIN as u64 && main_dist >= 0x80 {
                main_len = 1;
            }
        }

        if best_rep_len >= MATCH_LEN_MIN
            && (best_rep_len + 1 >= main_len as usize
                || (best_rep_len + 2 >= main_len as usize && main_dist >= (1 << 9))
                || (best_rep_len + 3 >= main_len as usize && main_dist >= (1 << 15)))
        {
            encoder.data.back = best_rep_index as _;
            encoder.skip(best_rep_len - 1);
            return best_rep_len as _;
        }

        if main_len < MATCH_LEN_MIN as _ || avail <= MATCH_LEN_MIN as _ {
            return 1;
        }
        // Get the next match. Test if it is better than the current match.
        // If so, encode the current byte as a literal.
        encoder.find_matches();
        let matches = encoder.lz.matches();
        if matches.count > 0 {
            let new_len = matches.len[matches.count as usize - 1];
            let new_dist = matches.dist[matches.count as usize - 1];

            if (new_len >= main_len && new_dist < main_dist)
                || (new_len == main_len + 1 && !change_pair(main_dist as _, new_dist as _))
                || new_len > main_len + 1
                || (new_len + 1 >= main_len
                    && main_len > MATCH_LEN_MIN as u64
                    && change_pair(new_dist as _, main_dist as _))
            {
                return 1;
            }
        }

        let limit = (main_len - 1).max(MATCH_LEN_MIN as _);
        for rep in 0..REPS {
            if encoder.lz.get_match_len(encoder.reps[rep], limit as i64) == limit as _ {
                return 1;
            }
        }

        encoder.data.back = (main_dist + REPS as i64) as _;
        encoder.skip((main_len - 2) as _);
        main_len
    }
}
