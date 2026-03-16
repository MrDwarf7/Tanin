use super::App;

impl App {
    #[inline(always)]
    pub fn update_grid_cols(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;

        let card_width = 24; // 22 + 2 margin
        let cols = (self.width.saturating_sub(4)) / card_width;
        self.grid_cols = if cols < 1 { 1 } else { cols };
    }

    pub fn get_sound_row_top(&self, index: usize) -> u16 {
        let mut current_y = 0;
        let card_height = 5; // card height
        let cols = self.grid_cols as usize;

        let filtered = self.get_filtered_sounds();
        let mut categories: Vec<String> =
            filtered.iter().map(|(_, s)| s.category.clone()).collect();
        categories.dedup();

        for category in categories {
            let cat_sounds: Vec<usize> = filtered
                .iter()
                .filter(|(_, s)| s.category == category)
                .map(|(i, _)| *i)
                .collect();

            // Header takes 2 rows
            current_y += 2;

            for chunk in cat_sounds.chunks(cols) {
                // Check if our index is in this chunk
                if chunk.contains(&index) {
                    return current_y;
                }
                // Height of row + 1 empty line
                current_y += card_height + 1;
            }
        }
        0
    }

    pub fn scroll_into_view(&mut self) {
        let card_height = 5;
        let viewport_height = self.height.saturating_sub(6); // 3 header + 3 footer
        let row_top = self.get_sound_row_top(self.cursor_pos);
        let row_bottom = row_top + card_height;

        // Ensure we see the header (2 rows above) if we scroll up
        let effective_top = row_top.saturating_sub(2);

        if effective_top < self.grid_scroll {
            self.grid_scroll = effective_top;
        } else if row_bottom > self.grid_scroll + viewport_height {
            self.grid_scroll = row_bottom.saturating_sub(viewport_height);
        }
    }

    pub fn get_visual_layout(&self) -> Vec<(usize, u16, u16)> {
        let mut layout = Vec::new();
        let mut current_row = 0;
        let cols = self.grid_cols as usize;

        let filtered = self.get_filtered_sounds();
        let mut categories: Vec<String> =
            filtered.iter().map(|(_, s)| s.category.clone()).collect();
        categories.dedup();

        for category in categories {
            let cat_sounds: Vec<usize> = filtered
                .iter()
                .filter(|(_, s)| s.category == category)
                .map(|(i, _)| *i)
                .collect();

            for (i, global_idx) in cat_sounds.iter().enumerate() {
                let col = i % cols;
                let row = current_row + (i / cols);
                layout.push((*global_idx, col as u16, row as u16));
            }

            let rows_in_cat = cat_sounds.len().div_ceil(cols);
            current_row += rows_in_cat;
        }
        layout
    }

    pub fn move_left(&mut self) {
        let filtered = self.get_filtered_sounds();
        if let Some(pos) = filtered.iter().position(|(i, _)| *i == self.cursor_pos) {
            if pos > 0 {
                self.cursor_pos = filtered[pos - 1].0;
                self.scroll_into_view();
            }
        } else if !filtered.is_empty() {
            self.cursor_pos = filtered[0].0;
            self.scroll_into_view();
        }
    }

    pub fn move_right(&mut self) {
        let filtered = self.get_filtered_sounds();
        if let Some(pos) = filtered.iter().position(|(i, _)| *i == self.cursor_pos) {
            if pos < filtered.len() - 1 {
                self.cursor_pos = filtered[pos + 1].0;
                self.scroll_into_view();
            }
        } else if !filtered.is_empty() {
            self.cursor_pos = filtered[0].0;
            self.scroll_into_view();
        }
    }

    pub fn move_up(&mut self) {
        let layout = self.get_visual_layout();

        // Safety: if cursor is hidden by filter, jump to first visible
        if !layout.iter().any(|(i, _, _)| *i == self.cursor_pos) {
            if let Some((first, _, _)) = layout.first() {
                self.cursor_pos = *first;
                self.scroll_into_view();
            }
            return;
        }

        if let Some((_, curr_col, curr_row)) = layout.iter().find(|(i, _, _)| *i == self.cursor_pos)
        {
            if *curr_row > 0 {
                let target_row = curr_row - 1;
                // Find sound in target row closest to curr_col
                if let Some((best_idx, _, _)) = layout
                    .iter()
                    .filter(|(_, _, r)| *r == target_row)
                    .min_by_key(|(_, c, _)| (*c as i32 - *curr_col as i32).abs())
                {
                    self.cursor_pos = *best_idx;
                    self.scroll_into_view();
                }
            }
        }
    }

    pub fn move_down(&mut self) {
        let layout = self.get_visual_layout();

        // Safety
        if !layout.iter().any(|(i, _, _)| *i == self.cursor_pos) {
            if let Some((first, _, _)) = layout.first() {
                self.cursor_pos = *first;
                self.scroll_into_view();
            }
            return;
        }

        if let Some((_, curr_col, curr_row)) = layout.iter().find(|(i, _, _)| *i == self.cursor_pos)
        {
            let target_row = curr_row + 1;
            // Find sound in target row closest to curr_col
            if let Some((best_idx, _, _)) = layout
                .iter()
                .filter(|(_, _, r)| *r == target_row)
                .min_by_key(|(_, c, _)| (*c as i32 - *curr_col as i32).abs())
            {
                self.cursor_pos = *best_idx;
                self.scroll_into_view();
            }
        }
    }

    pub fn scroll_grid(&mut self, delta: i32) {
        let new_scroll = self.grid_scroll as i32 + delta;
        if new_scroll < 0 {
            self.grid_scroll = 0;
            return;
        }

        let filtered = self.get_filtered_sounds();
        if filtered.is_empty() {
            self.grid_scroll = 0;
            return;
        }

        // Calculate max scroll
        let last_idx = filtered.last().map(|(idx, _)| *idx).unwrap_or(0);
        // get_sound_row_top gives top Y relative to start.
        // + 5 (card height) + 1 (margin)
        let content_height = self.get_sound_row_top(last_idx) as i32 + 6;
        let viewport_height = self.height.saturating_sub(6) as i32;

        let max_scroll = if content_height > viewport_height {
            (content_height - viewport_height) as u16
        } else {
            0
        };

        self.grid_scroll = (new_scroll as u16).min(max_scroll);
    }

    pub fn validate_cursor_position(&mut self) {
        let filtered = self.get_filtered_sounds();
        if filtered.is_empty() {
            return;
        }

        if !filtered.iter().any(|(i, _)| *i == self.cursor_pos) {
            if let Some((first, _)) = filtered.first() {
                self.cursor_pos = *first;
                self.scroll_into_view();
            }
        }
    }
}
