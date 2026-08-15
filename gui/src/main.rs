use std::collections::HashMap;
use std::num::NonZero;

use beeriokartbracket::{
    BracketRoundView, BracketSetId, BracketSetView, BracketView, Config, ParticipantId,
    ParticipantView, Placement, PoolResultView, PoolView, RaceId, RaceRuleset, RaceView,
    RegistrationView, Tournament, TournamentError, TournamentView,
};
use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([980.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Beerio Kart Invitational",
        options,
        Box::new(|cc| Ok(Box::new(TournamentApp::new(cc)))),
    )
}

/// A tournament-official action, collected while drawing and applied after the
/// frame so rendering only ever reads the (owned) view snapshot.
enum Action {
    Add(String),
    AddMany(usize),
    Remove(ParticipantId),
    Start,
    NextRace(Vec<(ParticipantId, Option<Placement>)>),
    EditRace(RaceId, Vec<(ParticipantId, Option<Placement>)>),
    UpdateBracketSet(BracketSetId, usize, Vec<(ParticipantId, Option<Placement>)>),
    Next,
}

struct TournamentApp {
    tournament: Tournament,
    new_name: String,
    add_count: usize,
    pool_rounds: usize,
    bracket_size: usize,
    races_per_round: usize,
    // Place text entry for the active pool race, keyed by racer (empty = unset).
    placement_inputs: HashMap<ParticipantId, String>,
    // Place text entry for correcting a completed race, keyed by (race, racer).
    race_edits: HashMap<(RaceId, ParticipantId), String>,
    // Place text entry for a bracket set's races, keyed by (set, race index, racer).
    bracket_edits: HashMap<(BracketSetId, usize, ParticipantId), String>,
    // Last-frame measured heat-card heights, for stable tree layout.
    bracket_heights: HashMap<BracketSetId, f32>,
    show_scores: bool,
    status: String,
    error: Option<String>,
    logo: Option<egui::TextureHandle>,
    background: Option<egui::TextureHandle>,
}

impl Default for TournamentApp {
    fn default() -> Self {
        Self {
            tournament: Tournament::default(),
            new_name: String::new(),
            add_count: 16,
            pool_rounds: 8,
            bracket_size: 16,
            races_per_round: 3,
            placement_inputs: HashMap::new(),
            race_edits: HashMap::new(),
            bracket_edits: HashMap::new(),
            bracket_heights: HashMap::new(),
            show_scores: false,
            status: String::new(),
            error: None,
            logo: None,
            background: None,
        }
    }
}

impl TournamentApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_theme(&cc.egui_ctx);
        install_fonts(&cc.egui_ctx);
        Self {
            logo: load_logo(&cc.egui_ctx),
            background: load_background(&cc.egui_ctx),
            ..Default::default()
        }
    }
}

impl eframe::App for TournamentApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let view = self.tournament.view();
        let mut action: Option<Action> = None;

        // Tile the beer texture behind every panel (repeat at native size).
        if let Some(background) = &self.background {
            let screen = ctx.screen_rect();
            let tile = background.size_vec2();
            let uv = egui::Rect::from_min_max(
                egui::pos2(0.0, 0.0),
                egui::pos2(screen.width() / tile.x, screen.height() / tile.y),
            );
            ctx.layer_painter(egui::LayerId::background()).image(
                background.id(),
                screen,
                uv,
                egui::Color32::WHITE,
            );
        }

        egui::TopBottomPanel::top("title").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.vertical_centered(|ui| {
                if let Some(logo) = &self.logo {
                    let size = logo.size_vec2();
                    let scale = 88.0 / size.y;
                    let sized = egui::load::SizedTexture::new(logo.id(), size * scale);
                    ui.add(egui::Image::new(sized));
                }
                banner(ui, phase_name(&view), 22.0);
            });
            ui.add_space(6.0);
        });

        if !self.status.is_empty() {
            egui::TopBottomPanel::bottom("status")
                .frame(
                    egui::Frame::none()
                        .fill(AMBER)
                        .inner_margin(egui::Margin::symmetric(10.0, 6.0)),
                )
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new(self.status.clone())
                            .color(egui::Color32::BLACK)
                            .strong()
                            .size(16.0),
                    );
                });
        }

        if matches!(&view, TournamentView::Registration(_)) {
            egui::SidePanel::right("setup")
                .resizable(false)
                .exact_width(250.0)
                .frame(solid_panel_frame())
                .show(ctx, |ui| {
                    self.setup_panel_ui(ui, &mut action);
                });
        }

        if self.show_scores
            && let TournamentView::Pools((pool, _)) = &view
        {
            egui::SidePanel::right("scoreboard")
                .resizable(false)
                .exact_width(280.0)
                .frame(solid_panel_frame())
                .show(ctx, |ui| {
                    self.scoreboard_sidebar(ui, pool);
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| match view {
                TournamentView::Registration(reg) => self.registration_ui(ui, reg, &mut action),
                TournamentView::Pools(pool) => self.pools_ui(ui, pool.0, pool.1, &mut action),
                TournamentView::Bracket(bracket) => self.bracket_ui(ui, bracket, &mut action),
                TournamentView::Gauntlet => {
                    ui.label("Not yet implemented.");
                }
                TournamentView::Complete => {
                    ui.label("The tournament is complete.");
                }
            });
        });

        if let Some(action) = action {
            self.apply(action);
        }

        self.error_popup(ctx);
    }
}

impl TournamentApp {
    fn registration_ui(
        &mut self,
        ui: &mut egui::Ui,
        reg: RegistrationView,
        action: &mut Option<Action>,
    ) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Name:");
            let resp = ui.add(egui::TextEdit::singleline(&mut self.new_name).desired_width(260.0));
            let entered = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("Add").clicked() || entered) && !self.new_name.trim().is_empty() {
                *action = Some(Action::Add(self.new_name.clone()));
            }
        });

        ui.horizontal(|ui| {
            ui.label("Bulk add:");
            ui.add(egui::DragValue::new(&mut self.add_count).range(1..=128));
            if ui.button("Add numbered players").clicked() {
                *action = Some(Action::AddMany(self.add_count));
            }
        });

        ui.add_space(4.0);
        ui.separator();
        outlined_text(
            ui,
            &format!("Participants ({})", reg.participants.len()),
            24.0,
            AMBER,
        );

        let count = reg.participants.len();
        if count == 0 {
            ui.label("No participants yet — add some to get started.");
            return;
        }

        // Balanced columns so the whole roster stays on screen without scrolling.
        let col_count = count.div_ceil(12).clamp(1, 4);
        let rows = count.div_ceil(col_count);
        ui.columns(col_count, |cols| {
            for (i, participant) in reg.participants.iter().enumerate() {
                let col = &mut cols[i / rows];
                col.horizontal(|ui| {
                    if ui.button("X").on_hover_text("Remove").clicked() {
                        *action = Some(Action::Remove(participant.id));
                    }
                    outlined_text(ui, &format!("{}. {}", i + 1, participant.name), 18.0, CREAM);
                });
            }
        });
    }

    fn setup_panel_ui(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        ui.add_space(6.0);
        ui.heading("Setup");
        ui.separator();
        ui.add_space(6.0);

        egui::Grid::new("config")
            .num_columns(2)
            .spacing([10.0, 14.0])
            .show(ui, |ui| {
                ui.label("Pool rounds");
                ui.add(egui::DragValue::new(&mut self.pool_rounds).range(1..=32));
                ui.end_row();
                ui.label("Bracket size");
                ui.add(egui::DragValue::new(&mut self.bracket_size).range(1..=64));
                ui.end_row();
                ui.label("Races per heat");
                ui.add(egui::DragValue::new(&mut self.races_per_round).range(1..=9));
                ui.end_row();
            });

        ui.add_space(18.0);
        let start = ui.add_sized(
            [ui.available_width(), 46.0],
            egui::Button::new(
                egui::RichText::new("Start tournament")
                    .size(20.0)
                    .strong()
                    .color(egui::Color32::BLACK),
            )
            .fill(AMBER),
        );
        if start.clicked() {
            *action = Some(Action::Start);
        }
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("Config is applied when you start.")
                .small()
                .weak(),
        );
    }

    fn pending_config(&self, seed: u64) -> Option<Config> {
        Some(Config {
            pool_rounds: NonZero::new(self.pool_rounds)?,
            bracket_size: NonZero::new(self.bracket_size)?,
            bracket_races_per_round: NonZero::new(self.races_per_round)?,
            seed,
        })
    }

    fn error_popup(&mut self, ctx: &egui::Context) {
        if self.error.is_none() {
            return;
        }

        let mut open = true;
        let mut dismissed = false;
        egui::Window::new(egui::RichText::new("⚠  Something went wrong").size(20.0))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_max_width(400.0);
                if let Some(message) = &self.error {
                    ui.label(egui::RichText::new(message).size(18.0));
                }
                ui.add_space(12.0);
                ui.vertical_centered(|ui| {
                    if ui
                        .add(egui::Button::new(egui::RichText::new("OK").strong()))
                        .clicked()
                    {
                        dismissed = true;
                    }
                });
            });

        if !open || dismissed {
            self.error = None;
        }
    }

    fn pools_ui(
        &mut self,
        ui: &mut egui::Ui,
        pool: PoolView,
        results: Option<PoolResultView>,
        action: &mut Option<Action>,
    ) {
        // Scoreboard toggle, top-right.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if ui.button("📊  Scoreboard").clicked() {
                self.show_scores = !self.show_scores;
            }
        });

        match &pool.current_race {
            Some(race) => {
                let round = pool.current_round + 1;
                let (emoji, kind) = match race.ruleset {
                    RaceRuleset::Beerio => ("🍺", "Beerio"),
                    RaceRuleset::Vanilla => ("🏁", "Vanilla"),
                };
                let racer_count = race.racers.len() as u8;

                // Pulsing border to make the active race pop.
                let t = ui.input(|i| i.time);
                let pulse = ((t * 2.5).sin() * 0.5 + 0.5) as f32;
                let border = lerp_color(AMBER, AMBER_BRIGHT, pulse);
                ui.ctx().request_repaint();

                ui.vertical_centered(|ui| {
                    egui::Frame::none()
                        .fill(CARD_BG)
                        .stroke(egui::Stroke::new(3.0_f32, border))
                        .rounding(10.0)
                        .show(ui, |ui| {
                            ui.set_width(420.0);

                            // Yellow title bar with the round number and ruleset.
                            egui::Frame::none()
                                .fill(AMBER)
                                .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                                .rounding(egui::Rounding {
                                    nw: 8.0,
                                    ne: 8.0,
                                    sw: 0.0,
                                    se: 0.0,
                                })
                                .show(ui, |ui| {
                                    ui.set_width(420.0);
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{emoji}  Round {round} · {kind}"
                                        ))
                                        .color(egui::Color32::BLACK)
                                        .font(title_font(24.0)),
                                    );
                                });

                            egui::Frame::none()
                                .inner_margin(egui::Margin::symmetric(12.0, 10.0))
                                .show(ui, |ui| {
                                    ui.scope(|ui| {
                                        ui.spacing_mut().item_spacing.y = 0.0;
                                        for (i, (participant, _)) in race.racers.iter().enumerate()
                                        {
                                            let buf = self
                                                .placement_inputs
                                                .entry(participant.id)
                                                .or_default();
                                            race_row(
                                                ui,
                                                participant.name.as_str(),
                                                buf,
                                                racer_count,
                                                ("place", participant.id),
                                                i,
                                            );
                                        }
                                    });

                                    ui.add_space(10.0);

                                    // Ties are allowed; just need every racer to have a place chosen.
                                    let ready = race.racers.iter().all(|(participant, _)| {
                                        matches!(
                                            self.placement_inputs
                                                .get(&participant.id)
                                                .and_then(|s| s.trim().parse::<u8>().ok()),
                                            Some(v) if (1..=racer_count).contains(&v)
                                        )
                                    });

                                    let next = ui.add_enabled(
                                        ready,
                                        egui::Button::new(
                                            egui::RichText::new("Next race ▶")
                                                .strong()
                                                .color(egui::Color32::BLACK),
                                        )
                                        .fill(AMBER),
                                    );
                                    if next.clicked() {
                                        let submitted: Vec<(ParticipantId, Option<Placement>)> =
                                            race.racers
                                                .iter()
                                                .map(|(participant, _)| {
                                                    let place = self
                                                        .placement_inputs
                                                        .get(&participant.id)
                                                        .and_then(|s| s.trim().parse::<u8>().ok())
                                                        .and_then(|v| Placement::new(v).ok());
                                                    (participant.id, place)
                                                })
                                                .collect();
                                        *action = Some(Action::NextRace(submitted));
                                    }
                                });
                        });
                });
            }
            None => {
                ui.add_space(8.0);
                if ui
                    .add_sized(
                        [240.0, 46.0],
                        egui::Button::new(
                            egui::RichText::new("Proceed to bracket ▶")
                                .strong()
                                .color(egui::Color32::BLACK),
                        )
                        .fill(AMBER),
                    )
                    .clicked()
                {
                    *action = Some(Action::Next);
                }
            }
        }

        self.completed_races_ui(ui, &pool, action);

        if let Some(results) = results {
            ui.add_space(14.0);
            banner(ui, "Provisional standings", 22.0);
            ui.add_space(8.0);
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(0x2A, 0x22, 0x14))
                .stroke(egui::Stroke::new(1.5_f32, AMBER))
                .rounding(10.0)
                .inner_margin(egui::Margin::same(14.0))
                .show(ui, |ui| {
                    ui.columns(2, |cols| {
                        standings_column(
                            &mut cols[0],
                            "advancing",
                            "Advancing",
                            egui::Color32::from_rgb(0x7F, 0xC2, 0x4A),
                            &results.advanced,
                        );
                        standings_column(
                            &mut cols[1],
                            "eliminated",
                            "Eliminated",
                            egui::Color32::from_rgb(0xE0, 0x7B, 0x5A),
                            &results.eliminated,
                        );
                    });
                });
        }
    }

    fn scoreboard_sidebar(&self, ui: &mut egui::Ui, pool: &PoolView) {
        // Aggregate points and race counts from completed races.
        let mut totals: HashMap<ParticipantId, (String, usize, usize)> = HashMap::new();

        // Seed every participant so those who haven't raced yet still show (0 pts, 0 races).
        // At any moment the whole field sits across these three buckets.
        let roster = pool
            .remaining_racers_in_round
            .iter()
            .chain(pool.completed_racers_in_round.iter())
            .chain(
                pool.current_race
                    .iter()
                    .flat_map(|race| race.racers.iter().map(|(participant, _)| participant)),
            );
        for participant in roster {
            totals
                .entry(participant.id)
                .or_insert_with(|| (participant.name.clone(), 0, 0));
        }

        for (_, race, _) in &pool.completed_races {
            for (participant, place) in &race.racers {
                let entry = totals
                    .entry(participant.id)
                    .or_insert_with(|| (participant.name.clone(), 0, 0));
                entry.1 += place.map_or(0, |p| p.points());
                entry.2 += 1;
            }
        }
        let mut rows: Vec<(String, usize, usize)> = totals.into_values().collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        ui.label(
            egui::RichText::new("📊  Scoreboard")
                .color(AMBER)
                .strong()
                .size(20.0),
        );
        ui.separator();
        if rows.is_empty() {
            ui.label("No races completed yet.");
        } else {
            egui::Grid::new("scoreboard")
                .num_columns(3)
                .striped(true)
                .spacing([18.0, 6.0])
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Racer")
                            .color(AMBER)
                            .strong()
                            .size(17.0),
                    );
                    ui.label(
                        egui::RichText::new("Points")
                            .color(AMBER)
                            .strong()
                            .size(17.0),
                    );
                    ui.label(
                        egui::RichText::new("Races")
                            .color(AMBER)
                            .strong()
                            .size(17.0),
                    );
                    ui.end_row();
                    for (name, points, races) in &rows {
                        ui.label(egui::RichText::new(name.as_str()).color(CREAM).size(17.0));
                        ui.label(
                            egui::RichText::new(points.to_string())
                                .color(CREAM)
                                .size(17.0),
                        );
                        ui.label(
                            egui::RichText::new(races.to_string())
                                .color(CREAM)
                                .size(17.0),
                        );
                        ui.end_row();
                    }
                });
        }
    }

    fn completed_races_ui(
        &mut self,
        ui: &mut egui::Ui,
        pool: &PoolView,
        action: &mut Option<Action>,
    ) {
        if pool.completed_races.is_empty() {
            return;
        }

        ui.add_space(12.0);
        ui.separator();
        banner(ui, "Completed races (edit to fix a mistake)", 20.0);

        // Deterministic play order: by round, then race id (ids are monotonic).
        let mut races: Vec<&(RaceId, RaceView, usize)> = pool.completed_races.iter().collect();
        races.sort_by_key(|(id, _, round)| (*round, *id));

        // One row per round; races laid out horizontally within the row.
        for group in races.chunk_by(|a, b| a.2 == b.2) {
            let round = group[0].2;
            ui.add_space(8.0);
            banner(ui, &format!("Round {}", round + 1), 18.0);
            ui.add_space(2.0);
            ui.horizontal_top(|ui| {
                for (id, race, _) in group {
                    ui.scope(|ui| {
                        ui.set_opacity(0.85);
                        self.completed_race_card(ui, *id, race, action);
                    });
                }
            });
        }
    }

    fn completed_race_card(
        &mut self,
        ui: &mut egui::Ui,
        id: RaceId,
        race: &RaceView,
        action: &mut Option<Action>,
    ) {
        let (emoji, kind) = match race.ruleset {
            RaceRuleset::Beerio => ("🍺", "Beerio"),
            RaceRuleset::Vanilla => ("🏁", "Vanilla"),
        };
        let racer_count = race.racers.len() as u8;

        egui::Frame::none()
            .fill(CARD_BG)
            .stroke(egui::Stroke::new(1.0_f32, AMBER))
            .rounding(8.0)
            .inner_margin(egui::Margin::symmetric(10.0, 8.0))
            .show(ui, |ui| {
                // The card sits inside `horizontal_top`, so force its body vertical.
                ui.vertical(|ui| {
                    ui.set_width(320.0);
                    ui.label(
                        egui::RichText::new(format!("{emoji}  {kind}"))
                            .color(AMBER)
                            .font(title_font(18.0)),
                    );

                    ui.scope(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (i, (participant, place)) in race.racers.iter().enumerate() {
                            let buf =
                                self.race_edits
                                    .entry((id, participant.id))
                                    .or_insert_with(|| match place {
                                        Some(p) => p.placement().to_string(),
                                        None => String::new(),
                                    });
                            race_row(
                                ui,
                                participant.name.as_str(),
                                buf,
                                racer_count,
                                ("edit", id, participant.id),
                                i,
                            );
                        }
                    });

                    // Ties are allowed; just need every racer to have a place chosen.
                    let ready = race.racers.iter().all(|(participant, _)| {
                        matches!(
                            self.race_edits
                                .get(&(id, participant.id))
                                .and_then(|s| s.trim().parse::<u8>().ok()),
                            Some(v) if (1..=racer_count).contains(&v)
                        )
                    });

                    if ui.add_enabled(ready, egui::Button::new("Save")).clicked() {
                        let results: Vec<(ParticipantId, Option<Placement>)> = race
                            .racers
                            .iter()
                            .map(|(participant, _)| {
                                let place = self
                                    .race_edits
                                    .get(&(id, participant.id))
                                    .and_then(|s| s.trim().parse::<u8>().ok())
                                    .and_then(|v| Placement::new(v).ok());
                                (participant.id, place)
                            })
                            .collect();
                        *action = Some(Action::EditRace(id, results));
                    }
                });
            });
    }

    fn apply(&mut self, action: Action) {
        let outcome: Result<String, _> = match action {
            Action::Add(name) => {
                let result = self
                    .tournament
                    .add_participant(name.trim())
                    .map(|_| "Added participant".to_owned());
                if result.is_ok() {
                    self.new_name.clear();
                }
                result
            }
            Action::AddMany(count) => {
                let start = match self.tournament.view() {
                    TournamentView::Registration(reg) => reg.participants.len(),
                    _ => 0,
                };
                let mut result = Ok(format!("Added {count} participants"));
                for i in 1..=count {
                    if let Err(e) = self
                        .tournament
                        .add_participant(&format!("Player{}", start + i))
                    {
                        result = Err(e);
                        break;
                    }
                }
                result
            }
            Action::Remove(id) => self
                .tournament
                .remove_participant(id)
                .map(|_| "Removed participant".to_owned()),
            Action::Start => {
                // Config is settable only during registration, so push it just before advancing.
                let seed = match self.tournament.view() {
                    TournamentView::Registration(reg) => reg.config.seed,
                    _ => 0,
                };
                if let Some(config) = self.pending_config(seed) {
                    let _ = self.tournament.set_config(config);
                }
                match self.tournament.next_phase() {
                    Ok(()) => {
                        // Draw the first pool race automatically.
                        let _ = self.tournament.advance_pools();
                        Ok("Tournament started".to_owned())
                    }
                    Err(e) => Err(e),
                }
            }
            Action::NextRace(results) => match self.tournament.update_active_race(results) {
                Ok(_) => {
                    self.placement_inputs.clear();
                    self.tournament.advance_pools().map(|complete| {
                        if complete {
                            "Pool complete — proceed to the bracket.".to_owned()
                        } else {
                            "Next race ready.".to_owned()
                        }
                    })
                }
                Err(e) => Err(e),
            },
            Action::EditRace(id, results) => self
                .tournament
                .update_completed_race(id, results)
                .map(|_| "Race corrected".to_owned()),
            Action::UpdateBracketSet(id, race_index, results) => self
                .tournament
                .update_bracket_set(id, race_index, results)
                .map(|completed| {
                    if completed {
                        "Heat complete.".to_owned()
                    } else {
                        "Race result saved.".to_owned()
                    }
                }),
            Action::Next => self
                .tournament
                .next_phase()
                .map(|_| "Advanced to next phase".to_owned()),
        };

        match outcome {
            Ok(message) => self.status = message,
            Err(e) => {
                self.status.clear();
                self.error = Some(describe_error(&e));
            }
        }
    }
}

/// One side of the provisional standings: a titled, striped, ranked list.
fn standings_column(
    ui: &mut egui::Ui,
    id: &str,
    title: &str,
    accent: egui::Color32,
    rows: &[(ParticipantView, usize)],
) {
    ui.label(
        egui::RichText::new(format!("{title} ({})", rows.len()))
            .color(accent)
            .strong()
            .size(19.0),
    );
    ui.add_space(6.0);
    egui::Grid::new(id)
        .num_columns(3)
        .striped(true)
        .spacing([14.0, 7.0])
        .show(ui, |ui| {
            for (rank, (participant, score)) in rows.iter().enumerate() {
                ui.label(
                    egui::RichText::new(format!("{}.", rank + 1))
                        .color(AMBER)
                        .size(16.0),
                );
                ui.label(
                    egui::RichText::new(participant.name.as_str())
                        .color(CREAM)
                        .size(16.0),
                );
                ui.label(
                    egui::RichText::new(format!("{score} pts"))
                        .color(CREAM)
                        .strong()
                        .size(16.0),
                );
                ui.end_row();
            }
        });
}

impl TournamentApp {
    fn bracket_ui(&mut self, ui: &mut egui::Ui, bracket: BracketView, action: &mut Option<Action>) {
        banner(ui, "Bracket", 18.0);
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(
                "Enter a place for each racer in every race; results save as you go.",
            )
            .small()
            .weak(),
        );
        ui.add_space(10.0);

        let active = bracket.active_set;
        egui::ScrollArea::horizontal()
            .id_salt("bracket_scroll")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                banner(ui, "Winners", 20.0);
                ui.add_space(8.0);
                self.draw_bracket_section(ui, &bracket.winners, false, active, action);

                ui.add_space(28.0);
                banner(ui, "Losers", 20.0);
                ui.add_space(8.0);
                self.draw_bracket_section(ui, &bracket.losers, true, active, action);
            });
    }

    /// Draws one bracket half as columns of heat cards with connector elbows.
    /// Seeded heats render as an editable racer x race grid that autosaves; empty
    /// heats show a dashed placeholder.
    fn draw_bracket_section(
        &mut self,
        ui: &mut egui::Ui,
        rounds: &[BracketRoundView],
        is_losers: bool,
        active_set: Option<BracketSetId>,
        action: &mut Option<Action>,
    ) {
        if rounds.is_empty() {
            ui.label(egui::RichText::new("No heats yet.").color(CREAM));
            return;
        }

        // Pulsing accent for the heat awaiting results.
        let pulse = ((ui.input(|i| i.time) * 2.5).sin() * 0.5 + 0.5) as f32;
        let active_border = lerp_color(AMBER, AMBER_BRIGHT, pulse);
        if active_set.is_some() {
            ui.ctx().request_repaint();
        }

        // Every heat in the bracket shares the same race count.
        let n_races = rounds
            .iter()
            .flat_map(|round| round.sets.iter())
            .map(|(_, set)| set.races.len())
            .next()
            .unwrap_or(0);
        let card_w =
            2.0 * BRACKET_PAD + BRACKET_NAME_COL_W + n_races as f32 * (BRACKET_GRID_COL_W + 6.0);

        // Card heights: measured last frame when available (heat grids are
        // theme-sized), with an estimate as the first-paint fallback.
        let estimate = |set: &BracketSetView| -> f32 {
            if set.is_ready {
                let rows = set.racers.len().max(1) as f32;
                2.0 * BRACKET_PAD
                    + BRACKET_GRID_TITLE_H
                    + BRACKET_GRID_HEAD_H
                    + rows * BRACKET_GRID_ROW_H
            } else {
                BRACKET_PAD
                    + BRACKET_HEADER_H
                    + set.expected_size as f32 * BRACKET_LINE_H
                    + BRACKET_PAD
            }
        };
        let heights: Vec<Vec<f32>> = rounds
            .iter()
            .map(|round| {
                round
                    .sets
                    .iter()
                    .map(|(id, set)| {
                        if set.is_ready {
                            self.bracket_heights
                                .get(id)
                                .copied()
                                .unwrap_or_else(|| estimate(set))
                        } else {
                            estimate(set)
                        }
                    })
                    .collect()
            })
            .collect();

        // Canvas is as tall as the round that needs the most vertical space.
        let mut content_h: f32 = 0.0;
        for hs in &heights {
            let h: f32 = BRACKET_ROW_GAP + hs.iter().map(|x| x + BRACKET_ROW_GAP).sum::<f32>();
            content_h = content_h.max(h);
        }
        let total_h = BRACKET_COL_HEADER + content_h;
        let total_w = rounds.len() as f32 * (card_w + BRACKET_COL_GAP);

        let (canvas, _) =
            ui.allocate_exact_size(egui::vec2(total_w, total_h), egui::Sense::hover());

        // Position every card first; connectors need neighbouring columns' rects.
        let mut round_rects: Vec<Vec<egui::Rect>> = Vec::with_capacity(rounds.len());
        for (r, round) in rounds.iter().enumerate() {
            let x = canvas.min.x + r as f32 * (card_w + BRACKET_COL_GAP) + BRACKET_COL_GAP * 0.5;
            let n = round.sets.len().max(1);
            let mut rects = Vec::with_capacity(round.sets.len());
            for (j, &h) in heights[r].iter().enumerate() {
                let cy =
                    canvas.min.y + BRACKET_COL_HEADER + content_h * (j as f32 + 0.5) / n as f32;
                rects.push(egui::Rect::from_min_size(
                    egui::pos2(x, cy - h * 0.5),
                    egui::vec2(card_w, h),
                ));
            }
            round_rects.push(rects);
        }

        // Connectors and column titles paint under the cards.
        let painter = ui.painter_at(canvas);
        let wire = egui::Stroke::new(1.5_f32, AMBER);
        for r in 0..rounds.len().saturating_sub(1) {
            let cur = &round_rects[r];
            let next = &round_rects[r + 1];
            if next.is_empty() || cur.len() != next.len() * 2 {
                continue;
            }
            for (j, child) in cur.iter().enumerate() {
                let parent = next[j / 2];
                let mid_x = child.right() + BRACKET_COL_GAP * 0.5;
                let cy = child.center().y;
                let py = parent.center().y;
                painter.line_segment([egui::pos2(child.right(), cy), egui::pos2(mid_x, cy)], wire);
                painter.line_segment([egui::pos2(mid_x, cy), egui::pos2(mid_x, py)], wire);
                painter.line_segment([egui::pos2(mid_x, py), egui::pos2(parent.left(), py)], wire);
            }
        }
        for (r, round) in rounds.iter().enumerate() {
            let cx = canvas.min.x
                + r as f32 * (card_w + BRACKET_COL_GAP)
                + BRACKET_COL_GAP * 0.5
                + card_w * 0.5;
            paint_chip_text(
                &painter,
                egui::pos2(cx, canvas.min.y + BRACKET_COL_HEADER * 0.5),
                &round_title(r, round, is_losers),
            );
        }

        // Cards on top: editable grids for seeded heats, placeholders otherwise.
        let mut measured: Vec<(BracketSetId, f32)> = Vec::new();
        for (r, round) in rounds.iter().enumerate() {
            for (j, (id, set)) in round.sets.iter().enumerate() {
                let rect = round_rects[r][j];
                if set.is_ready {
                    let is_active = active_set == Some(*id);
                    let h = self.bracket_grid_card(
                        ui,
                        rect,
                        j,
                        *id,
                        set,
                        is_active,
                        active_border,
                        action,
                    );
                    measured.push((*id, h));
                } else {
                    let lines: Vec<String> =
                        (0..set.expected_size).map(|_| "—".to_owned()).collect();
                    paint_heat_card(&painter, rect, j, set, &lines, false, active_border);
                }
            }
        }

        // Feed measured heights back for next frame; repaint once if the layout shifted.
        let mut shifted = false;
        for (id, h) in measured {
            let changed = match self.bracket_heights.insert(id, h) {
                Some(old) => (old - h).abs() > 0.5,
                None => true,
            };
            shifted |= changed;
        }
        if shifted {
            ui.ctx().request_repaint();
        }
    }

    /// One heat as an editable grid: a row per racer, a place dropdown per race.
    /// A race is autosaved whenever its entered places differ from the stored ones.
    #[allow(clippy::too_many_arguments)]
    fn bracket_grid_card(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        index: usize,
        set_id: BracketSetId,
        set: &BracketSetView,
        is_active: bool,
        active_border: egui::Color32,
        action: &mut Option<Action>,
    ) -> f32 {
        let racer_count = set.racers.len() as u8;
        let (stroke_w, stroke_col) = if is_active {
            (2.5_f32, active_border)
        } else {
            (1.5_f32, AMBER)
        };
        let header_color = if is_active { active_border } else { AMBER };

        let resp = ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
            egui::Frame::none()
                .fill(CARD_BG)
                .stroke(egui::Stroke::new(stroke_w, stroke_col))
                .rounding(8.0)
                .inner_margin(egui::Margin::same(BRACKET_PAD))
                .show(ui, |ui| {
                    ui.set_width(rect.width() - 2.0 * BRACKET_PAD);
                    // Compact, predictable widget metrics so the grid stays tight.
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                    ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                    ui.spacing_mut().interact_size = egui::vec2(52.0, 22.0);

                    // The current round stays editable but is visually flagged.
                    let current = set.current_race_index;

                    ui.label(
                        egui::RichText::new(format!(
                            "Heat {} · {}/{}",
                            index + 1,
                            set.racers.len(),
                            set.expected_size
                        ))
                        .color(header_color)
                        .font(title_font(16.0)),
                    );

                    // Column header: Player, then one column per race with its ruleset.
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [BRACKET_NAME_COL_W, 18.0],
                            egui::Label::new(
                                egui::RichText::new("Player")
                                    .color(AMBER)
                                    .font(title_font(14.0)),
                            ),
                        );
                        for (r, race) in set.races.iter().enumerate() {
                            let emoji = match race.ruleset {
                                RaceRuleset::Beerio => "🍺",
                                RaceRuleset::Vanilla => "🏁",
                            };
                            let label = format!("R{} {}", r + 1, emoji);
                            let text = if r == current {
                                egui::RichText::new(label)
                                    .color(egui::Color32::BLACK)
                                    .background_color(AMBER_BRIGHT)
                                    .font(title_font(14.0))
                            } else {
                                egui::RichText::new(label)
                                    .color(AMBER)
                                    .font(title_font(14.0))
                            };
                            ui.add_sized([BRACKET_GRID_COL_W, 18.0], egui::Label::new(text));
                        }
                    });

                    // One row per racer; the roster order is shared across the heat's races.
                    for row in 0..set.racers.len() {
                        ui.horizontal(|ui| {
                            ui.add_sized(
                                [BRACKET_NAME_COL_W, 26.0],
                                egui::Label::new(
                                    egui::RichText::new(set.racers[row].name.as_str())
                                        .color(player_color(row))
                                        .font(mario_font(18.0)),
                                ),
                            );
                            for (r, race) in set.races.iter().enumerate() {
                                let participant = &race.racers[row].0;
                                let existing = race.racers[row].1;
                                let buf = self
                                    .bracket_edits
                                    .entry((set_id, r, participant.id))
                                    .or_insert_with(|| {
                                        existing
                                            .map_or(String::new(), |p| p.placement().to_string())
                                    });
                                if r == current {
                                    ui.scope(|ui| {
                                        let v = ui.visuals_mut();
                                        v.widgets.inactive.weak_bg_fill = CURRENT_COL_TINT;
                                        v.widgets.inactive.bg_fill = CURRENT_COL_TINT;
                                        v.widgets.hovered.weak_bg_fill = CURRENT_COL_TINT;
                                        place_input(
                                            ui,
                                            ("bracket", set_id, r, participant.id),
                                            buf,
                                            racer_count,
                                        );
                                    });
                                } else {
                                    place_input(
                                        ui,
                                        ("bracket", set_id, r, participant.id),
                                        buf,
                                        racer_count,
                                    );
                                }
                            }
                        });
                    }

                    // Autosave: submit any race whose entered places differ from the stored ones.
                    for (r, race) in set.races.iter().enumerate() {
                        let mut differs = false;
                        let results: Vec<(ParticipantId, Option<Placement>)> = race
                            .racers
                            .iter()
                            .map(|(participant, existing)| {
                                let entered = self
                                    .bracket_edits
                                    .get(&(set_id, r, participant.id))
                                    .and_then(|s| s.trim().parse::<u8>().ok())
                                    .and_then(|v| Placement::new(v).ok());
                                if entered != *existing {
                                    differs = true;
                                }
                                (participant.id, entered)
                            })
                            .collect();
                        if differs {
                            *action = Some(Action::UpdateBracketSet(set_id, r, results));
                        }
                    }
                });
        });
        resp.response.rect.height()
    }
}

// Layout constants shared by the section layout and the card painter.
const BRACKET_COL_GAP: f32 = 78.0;
const BRACKET_COL_HEADER: f32 = 36.0;
const BRACKET_HEADER_H: f32 = 28.0;
const BRACKET_LINE_H: f32 = 22.0;
const BRACKET_PAD: f32 = 11.0;
const BRACKET_ROW_GAP: f32 = 22.0;
// Editable heat-grid metrics.
const BRACKET_NAME_COL_W: f32 = 140.0;
const BRACKET_GRID_COL_W: f32 = 72.0;
const BRACKET_GRID_TITLE_H: f32 = 24.0;
const BRACKET_GRID_HEAD_H: f32 = 22.0;
const BRACKET_GRID_ROW_H: f32 = 30.0;
// Fill behind the current round's place dropdowns.
const CURRENT_COL_TINT: egui::Color32 = egui::Color32::from_rgb(0x8A, 0x6A, 0x24);

fn round_title(index: usize, round: &BracketRoundView, is_losers: bool) -> String {
    match (is_losers, round.from_wb_round) {
        (_, Some(wb)) => format!("Intake · W{}", wb + 1),
        (true, None) => "Consolidation".to_owned(),
        (false, None) => format!("Round {}", index + 1),
    }
}

/// A centered label sitting on a dark rounded chip (readable over the texture).
fn paint_chip_text(painter: &egui::Painter, center: egui::Pos2, text: &str) {
    let galley = painter.layout_no_wrap(text.to_owned(), title_font(18.0), AMBER);
    let pad = egui::vec2(8.0, 3.0);
    let rect = egui::Rect::from_center_size(center, galley.size() + pad * 2.0);
    painter.rect_filled(
        rect,
        egui::Rounding::same(5.0),
        egui::Color32::from_rgb(0x1C, 0x16, 0x0E),
    );
    painter.galley(rect.min + pad, galley, AMBER);
}

fn paint_heat_card(
    painter: &egui::Painter,
    rect: egui::Rect,
    index: usize,
    set: &BracketSetView,
    lines: &[String],
    is_active: bool,
    active_border: egui::Color32,
) {
    let rounding = egui::Rounding::same(8.0);
    painter.rect_filled(rect, rounding, CARD_BG);
    if is_active {
        // Bright pulsing border plus a soft outer ring to flag the set awaiting results.
        painter.rect_stroke(
            rect.expand(3.0),
            egui::Rounding::same(11.0),
            egui::Stroke::new(2.0_f32, active_border),
        );
        painter.rect_stroke(rect, rounding, egui::Stroke::new(3.0_f32, active_border));
    } else {
        painter.rect_stroke(rect, rounding, egui::Stroke::new(1.5_f32, AMBER));
    }

    let header_color = if is_active { active_border } else { AMBER };
    painter.text(
        rect.min + egui::vec2(BRACKET_PAD, BRACKET_PAD),
        egui::Align2::LEFT_TOP,
        format!(
            "Heat {} · {}/{}{}",
            index + 1,
            set.racers.len(),
            set.expected_size,
            if is_active { "  \u{25B6}" } else { "" }
        ),
        egui::FontId::proportional(17.0),
        header_color,
    );

    let sep_y = rect.min.y + BRACKET_PAD + BRACKET_HEADER_H - 4.0;
    painter.line_segment(
        [
            egui::pos2(rect.min.x + BRACKET_PAD, sep_y),
            egui::pos2(rect.max.x - BRACKET_PAD, sep_y),
        ],
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(0xA8, 0x80, 0x30)),
    );

    let empty = set.racers.is_empty();
    let name_color = if empty {
        egui::Color32::from_rgb(0x9B, 0x8A, 0x66)
    } else {
        CREAM
    };
    for (k, line) in lines.iter().enumerate() {
        painter.text(
            egui::pos2(
                rect.min.x + BRACKET_PAD,
                rect.min.y + BRACKET_PAD + BRACKET_HEADER_H + k as f32 * BRACKET_LINE_H,
            ),
            egui::Align2::LEFT_TOP,
            line,
            egui::FontId::proportional(18.0),
            name_color,
        );
    }
}

/// Human-readable, official-facing description for an error popup.
fn describe_error(err: &TournamentError) -> String {
    match err {
        TournamentError::NoParticipants => {
            "Add at least one participant before starting.".to_owned()
        }
        TournamentError::NotEnoughParticipants => "Not enough participants for a valid pool. \
             You need at least 6, and the total must split into 6-, 7-, or 8-player races."
            .to_owned(),
        TournamentError::NonExistentParticipant => "That participant no longer exists.".to_owned(),
        TournamentError::WrongPhase => {
            "That action isn't available in the current phase.".to_owned()
        }
        TournamentError::PoolsNotCompleted => {
            "Finish every pool race before advancing to the bracket.".to_owned()
        }
        TournamentError::RaceIsNotComplete => {
            "Enter a finishing place for every racer first.".to_owned()
        }
        TournamentError::ResultsDontMatchRace => {
            "The submitted results don't match the racers in this race.".to_owned()
        }
        TournamentError::InvalidPlacementValue => {
            "A finishing place is out of the valid range.".to_owned()
        }
        other => format!("Unexpected error: {other:?}"),
    }
}

fn outlined_text(ui: &mut egui::Ui, text: &str, size: f32, color: egui::Color32) {
    let font = egui::FontId::proportional(size);
    let outline = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::BLACK);
    let fill = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    let (rect, _) =
        ui.allocate_exact_size(fill.size() + egui::vec2(2.0, 2.0), egui::Sense::hover());
    let origin = rect.min + egui::vec2(1.0, 1.0);
    for dx in [-1.0_f32, 0.0, 1.0] {
        for dy in [-1.0_f32, 0.0, 1.0] {
            if dx != 0.0 || dy != 0.0 {
                ui.painter().galley(
                    origin + egui::vec2(dx, dy),
                    outline.clone(),
                    egui::Color32::BLACK,
                );
            }
        }
    }
    ui.painter().galley(origin, fill, color);
}

fn banner(ui: &mut egui::Ui, text: &str, size: f32) {
    egui::Frame::none()
        .fill(egui::Color32::from_rgb(0x1C, 0x16, 0x0E))
        .rounding(6.0)
        .inner_margin(egui::Margin::symmetric(10.0, 4.0))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .color(AMBER)
                    .font(title_font(size)),
            );
        });
}

fn solid_panel_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(egui::Color32::from_rgb(0x1C, 0x16, 0x0E))
        .inner_margin(egui::Margin::same(12.0))
}

fn ordinal(n: u8) -> String {
    let suffix = if (11..=13).contains(&(n % 100)) {
        "th"
    } else {
        match n % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    };
    format!("{n}{suffix}")
}

/// A place entry: an ordinal dropdown of `1..=count`, stored as a number in `buf`.
fn place_input(ui: &mut egui::Ui, id_source: impl std::hash::Hash, buf: &mut String, count: u8) {
    let current = buf.trim().parse::<u8>().ok();
    let mut selected = current.unwrap_or(0);
    egui::ComboBox::from_id_salt(id_source)
        .selected_text(current.map_or("—".to_owned(), ordinal))
        .width(64.0)
        .show_ui(ui, |ui| {
            for place in 1..=count {
                ui.selectable_value(&mut selected, place, ordinal(place));
            }
        });
    if selected != 0 && Some(selected) != current {
        *buf = selected.to_string();
    }
}

/// One banded table row: name on the left, place dropdown pinned to the right.
fn race_row(
    ui: &mut egui::Ui,
    name: &str,
    buf: &mut String,
    count: u8,
    id_source: impl std::hash::Hash,
    row: usize,
) {
    let band = if row.is_multiple_of(2) {
        egui::Color32::from_rgb(0x4E, 0x42, 0x2C)
    } else {
        egui::Color32::from_rgb(0x38, 0x2F, 0x1E)
    };
    egui::Frame::none()
        .fill(band)
        .inner_margin(egui::Margin::symmetric(10.0, 5.0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(name)
                        .color(player_color(row))
                        .font(mario_font(20.0)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    place_input(ui, id_source, buf, count);
                });
            });
        });
}

fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    egui::Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

fn phase_name(view: &TournamentView) -> &'static str {
    match view {
        TournamentView::Registration(_) => "Registration",
        TournamentView::Pools(_) => "Pools",
        TournamentView::Bracket(_) => "Bracket",
        TournamentView::Gauntlet => "Grand Finals Gauntlet",
        TournamentView::Complete => "Complete",
    }
}

// Logo palette: beer gold + cream foam on a dark, warm background.
const CREAM: egui::Color32 = egui::Color32::from_rgb(0xFA, 0xF3, 0xE0);
const AMBER: egui::Color32 = egui::Color32::from_rgb(0xF5, 0xA6, 0x23);
const AMBER_BRIGHT: egui::Color32 = egui::Color32::from_rgb(0xFF, 0xC6, 0x3C);
const CARD_BG: egui::Color32 = egui::Color32::from_rgb(0x45, 0x3A, 0x26);

fn apply_theme(ctx: &egui::Context) {
    use egui::Color32;

    // Fully transparent panels so the beer texture shows through in its exact colors.
    let bg = Color32::TRANSPARENT;
    let panel = Color32::from_rgb(0x3F, 0x35, 0x23);
    let widget = Color32::from_rgb(0x60, 0x51, 0x38);
    let rounding = egui::Rounding::same(6.0);

    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(CREAM);
    visuals.panel_fill = bg;
    visuals.window_fill = panel;
    visuals.window_rounding = rounding;
    visuals.extreme_bg_color = Color32::from_rgb(0x26, 0x1F, 0x14);
    visuals.faint_bg_color = Color32::from_rgb(0x3A, 0x30, 0x20);
    visuals.hyperlink_color = AMBER_BRIGHT;
    visuals.selection.bg_fill = Color32::from_rgb(0x7A, 0x53, 0x12);
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, CREAM);

    visuals.widgets.noninteractive.bg_fill = panel;
    visuals.widgets.noninteractive.weak_bg_fill = panel;
    visuals.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(1.0_f32, Color32::from_rgb(0x4A, 0x3E, 0x2A));
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, CREAM);
    visuals.widgets.noninteractive.rounding = rounding;

    visuals.widgets.inactive.bg_fill = widget;
    visuals.widgets.inactive.weak_bg_fill = widget;
    visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(1.5_f32, Color32::from_rgb(0xA8, 0x80, 0x30));
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, CREAM);
    visuals.widgets.inactive.rounding = rounding;

    visuals.widgets.hovered.bg_fill = AMBER;
    visuals.widgets.hovered.weak_bg_fill = AMBER;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.5_f32, AMBER_BRIGHT);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5_f32, Color32::BLACK);
    visuals.widgets.hovered.rounding = rounding;

    visuals.widgets.active.bg_fill = AMBER_BRIGHT;
    visuals.widgets.active.weak_bg_fill = AMBER_BRIGHT;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, CREAM);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, Color32::BLACK);
    visuals.widgets.active.rounding = rounding;

    visuals.widgets.open.bg_fill = widget;
    visuals.widgets.open.weak_bg_fill = widget;
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0_f32, CREAM);
    visuals.widgets.open.rounding = rounding;

    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 8.0);
    style.text_styles = [
        (
            egui::TextStyle::Heading,
            egui::FontId::new(26.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Body,
            egui::FontId::new(18.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Button,
            egui::FontId::new(18.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Monospace,
            egui::FontId::new(16.0, egui::FontFamily::Monospace),
        ),
        (
            egui::TextStyle::Small,
            egui::FontId::new(14.0, egui::FontFamily::Proportional),
        ),
    ]
    .into();
    ctx.set_style(style);
}

fn load_texture(
    ctx: &egui::Context,
    name: &str,
    bytes: &[u8],
    options: egui::TextureOptions,
) -> Option<egui::TextureHandle> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    let color = egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &image);
    Some(ctx.load_texture(name, color, options))
}

/// Registers the Mario display font as a named family alongside the defaults.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "mario".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/mario_font.ttf")),
    );
    fonts.font_data.insert(
        "rushblade".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/rushblade_font.ttf")),
    );

    // Custom families fall back to the defaults so missing glyphs (emoji,
    // punctuation) still render.
    let fallback = fonts
        .families
        .get(&egui::FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for name in ["mario", "rushblade"] {
        let mut family = vec![name.to_owned()];
        family.extend(fallback.iter().cloned());
        fonts
            .families
            .insert(egui::FontFamily::Name(name.into()), family);
    }

    ctx.set_fonts(fonts);
}

/// A `FontId` for the Mario display font at `size`.
fn mario_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name("mario".into()))
}

/// A `FontId` for the Rushblade title font at `size`.
fn title_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name("rushblade".into()))
}

/// Mario-Kart-style slot color for the racer in position `index` (0-based).
fn player_color(index: usize) -> egui::Color32 {
    const COLORS: [egui::Color32; 8] = [
        egui::Color32::from_rgb(0xE8, 0x40, 0x3C), // 1st  red
        egui::Color32::from_rgb(0x46, 0x78, 0xE8), // 2nd  blue
        egui::Color32::from_rgb(0x46, 0xC8, 0x54), // 3rd  green
        egui::Color32::from_rgb(0xF5, 0xD3, 0x30), // 4th  yellow
        egui::Color32::from_rgb(0xF2, 0x74, 0xC6), // 5th  pink
        egui::Color32::from_rgb(0xF0, 0x8C, 0x30), // 6th  orange
        egui::Color32::from_rgb(0x30, 0xC8, 0xC8), // 7th  teal
        egui::Color32::from_rgb(0xA9, 0x64, 0xE8), // 8th  purple
    ];
    COLORS[index % COLORS.len()]
}

fn load_logo(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    load_texture(
        ctx,
        "beerio-logo",
        include_bytes!("../assets/beerio_kart_logo.png"),
        egui::TextureOptions::LINEAR,
    )
}

fn load_background(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    load_texture(
        ctx,
        "beer-bg",
        include_bytes!("../assets/beer_texture.png"),
        egui::TextureOptions {
            wrap_mode: egui::TextureWrapMode::Repeat,
            ..egui::TextureOptions::LINEAR
        },
    )
}
