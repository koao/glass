//! 送信ルールパネル (別ウィンドウ)
//!
//! ルール一覧と選択ルールの編集フォームを並べる。
//! 手動送信は [送信] ボタン、定期/受信トリガは enabled チェックで作動する。

use eframe::egui;
use egui_phosphor::regular;

use crate::app::{GlassApp, MonitorState};
use crate::sender::{SendMode, SendModeKind, SendRule};
use crate::ui::theme;

pub fn draw(ctx: &egui::Context, app: &mut GlassApp) {
    if !app.ui_state.show_send_panel {
        return;
    }

    // ホストウィンドウの外にも移動できるよう、別 OS ウィンドウ (viewport) として表示する
    let viewport_id = egui::ViewportId::from_hash_of("send_panel_viewport");
    let builder = egui::ViewportBuilder::default()
        .with_title(app.t.send_panel_title)
        .with_inner_size([460.0, 320.0])
        .with_min_inner_size([380.0, 200.0]);

    let close_flag = std::cell::Cell::new(false);
    let content_height = std::cell::Cell::new(0.0_f32);
    ctx.show_viewport_immediate(viewport_id, builder, |viewport_ctx, _class| {
        // viewport トップレベルでは show_inside に置き換えられないため deprecated 警告を許容
        #[allow(deprecated)]
        let resp = egui::CentralPanel::default().show(viewport_ctx, |ui| {
            draw_body(ui, app);
            ui.min_rect().height()
        });
        content_height.set(resp.inner);

        if viewport_ctx.input(|i| i.viewport().close_requested()) {
            close_flag.set(true);
        }
    });

    if close_flag.get() {
        app.ui_state.show_send_panel = false;
    } else {
        // コンテンツ高に合わせて viewport の高さを自動調整 (幅はユーザー操作を尊重)
        // OS 側の丸め/微小レイアウト変動で振動しないよう、差が一定以上のときのみ送る
        let desired_h = (content_height.get() + 16.0).round();
        let current = ctx.input_for(viewport_id, |i| i.viewport_rect().size());
        if desired_h > 0.0 && (current.y.round() - desired_h).abs() >= 4.0 {
            ctx.send_viewport_cmd_to(
                viewport_id,
                egui::ViewportCommand::InnerSize(egui::vec2(current.x, desired_h)),
            );
        }
    }
}

fn draw_body(ui: &mut egui::Ui, app: &mut GlassApp) {
    let can_send = app.state == MonitorState::Running;

    // ヘッダー行: 追加ボタン
    ui.horizontal(|ui| {
        if ui
            .button(format!("{} {}", regular::PLUS, app.t.send_add_rule))
            .clicked()
        {
            let name = format!(
                "{} {}",
                app.t.send_new_rule_default_name,
                app.send_rules.len() + 1
            );
            app.send_rules.push(SendRule::new(name));
            app.ui_state.selected_send_rule_idx = Some(app.send_rules.len() - 1);
        }
        if !can_send {
            ui.label(
                egui::RichText::new(format!("{} {}", regular::WARNING, app.t.send_disabled_hint))
                    .color(theme::TEXT_MUTED)
                    .size(12.0),
            );
        }
    });

    ui.separator();

    if app.send_rules.is_empty() {
        ui.colored_label(theme::TEXT_MUTED, app.t.send_empty);
        return;
    }

    // ルール一覧
    draw_rule_list(ui, app, can_send);

    ui.separator();

    // 選択ルールの編集フォーム
    if let Some(idx) = app.ui_state.selected_send_rule_idx
        && idx < app.send_rules.len()
    {
        draw_rule_editor(ui, app, idx);
    }
}

fn draw_rule_list(ui: &mut egui::Ui, app: &mut GlassApp, can_send: bool) {
    // 現在のバッファ長 (有効化エッジでスキャンカーソルを末尾に合わせるため)
    let entries_len = app.buffer.entries().len();

    egui::ScrollArea::vertical()
        .max_height(160.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut send_idx: Option<usize> = None;
            let selected_idx = app.ui_state.selected_send_rule_idx;
            for (i, rule) in app.send_rules.iter_mut().enumerate() {
                let is_selected = selected_idx == Some(i);
                ui.horizontal(|ui| {
                    // 選択可能な名前ラベル (空なら "(no name)" フォールバック)
                    let label: &str = if rule.name.is_empty() {
                        "(no name)"
                    } else {
                        &rule.name
                    };
                    let resp = ui.selectable_label(is_selected, label);
                    if resp.clicked() {
                        app.ui_state.selected_send_rule_idx = Some(i);
                    }

                    ui.label(
                        egui::RichText::new(summarize_mode(rule, app.t)).color(theme::TEXT_MUTED),
                    );

                    // 右端: モード別コントロール
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let has_bytes = !rule.bytes().is_empty();
                        match rule.mode {
                            SendMode::Manual => {
                                // 手動: 送信ボタン
                                let enabled = can_send && has_bytes;
                                let btn = egui::Button::new(format!(
                                    "{} {}",
                                    regular::PAPER_PLANE_TILT,
                                    app.t.send_now
                                ));
                                if ui.add_enabled(enabled, btn).clicked() {
                                    send_idx = Some(i);
                                }
                            }
                            SendMode::Interval { .. } | SendMode::OnReceive { .. } => {
                                // 定期/受信トリガ: 有効トグル (受信中でなくても設定だけは可能)
                                let was_enabled = rule.enabled;
                                ui.checkbox(&mut rule.enabled, app.t.send_enabled_toggle);
                                if rule.enabled && !was_enabled {
                                    // 有効化の瞬間に走査カーソルを現在のバッファ末尾に合わせ、
                                    // 過去の受信を後追いでマッチさせないようにする
                                    rule.reset_execution_state(entries_len);
                                }
                            }
                        }
                    });
                });
            }
            if let Some(i) = send_idx {
                app.send_rule_now(i);
            }
        });
}

fn draw_rule_editor(ui: &mut egui::Ui, app: &mut GlassApp, idx: usize) {
    let mut duplicate = false;
    let mut delete = false;
    // モード内部フィールドの編集中は rule 全体を借用し直せないため、フラグで後処理
    let mut interval_changed = false;
    let mut on_receive_changed = false;

    {
        let rule = &mut app.send_rules[idx];

        ui.horizontal(|ui| {
            ui.label(app.t.send_rule_name);
            ui.add(egui::TextEdit::singleline(&mut rule.name).desired_width(260.0));
        });
        ui.add_space(10.0);

        ui.label(app.t.send_rule_data);
        let full_w = ui.available_width() - 8.0;
        let data_resp = ui.add(
            egui::TextEdit::singleline(&mut rule.data_text)
                .hint_text(app.t.send_rule_data_hint)
                .desired_width(full_w),
        );
        if data_resp.changed() {
            rule.refresh_bytes();
        }
        ui.colored_label(theme::TEXT_MUTED, format!("{} bytes", rule.bytes().len()));
        ui.add_space(10.0);

        // モード選択
        ui.label(app.t.send_rule_mode);
        let mut kind = rule.mode.kind();
        let prev_kind = kind;
        ui.horizontal(|ui| {
            ui.radio_value(&mut kind, SendModeKind::Manual, app.t.send_mode_manual);
            ui.radio_value(&mut kind, SendModeKind::Interval, app.t.send_mode_interval);
            ui.radio_value(
                &mut kind,
                SendModeKind::OnReceive,
                app.t.send_mode_on_receive,
            );
        });
        if kind != prev_kind {
            // モード切替時にデフォルト値で再構築
            rule.mode = match kind {
                SendModeKind::Manual => SendMode::Manual,
                SendModeKind::Interval => SendMode::Interval { period_ms: 500 },
                SendModeKind::OnReceive => SendMode::OnReceive {
                    pattern_text: String::new(),
                },
            };
            rule.refresh_on_receive_pattern();
            rule.reset_execution_state(0);
        }

        // モード依存フィールド
        match &mut rule.mode {
            SendMode::Manual => {}
            SendMode::Interval { period_ms } => {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(app.t.send_interval_label);
                    if ui
                        .add(
                            egui::DragValue::new(period_ms)
                                .range(1..=3_600_000u64)
                                .speed(10.0),
                        )
                        .changed()
                    {
                        interval_changed = true;
                    }
                    ui.label(app.t.send_interval_unit);
                });
            }
            SendMode::OnReceive { pattern_text } => {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(app.t.send_on_receive_label);
                    if ui
                        .add(
                            egui::TextEdit::singleline(pattern_text)
                                .hint_text(app.t.send_on_receive_hint)
                                .desired_width(240.0),
                        )
                        .changed()
                    {
                        on_receive_changed = true;
                    }
                });
            }
        }

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui
                .button(format!("{} {}", regular::COPY, app.t.send_duplicate))
                .clicked()
            {
                duplicate = true;
            }
            if ui
                .button(format!("{} {}", regular::TRASH, app.t.send_delete))
                .clicked()
            {
                delete = true;
            }
        });
    }

    // フォーム編集後の後処理 (rule を別途可変借用)
    if interval_changed || on_receive_changed {
        let rule = &mut app.send_rules[idx];
        if on_receive_changed {
            rule.refresh_on_receive_pattern();
        }
        rule.reset_execution_state(0);
    }

    if duplicate {
        let cloned = app.send_rules[idx].clone();
        app.send_rules.insert(idx + 1, cloned);
        app.ui_state.selected_send_rule_idx = Some(idx + 1);
    }
    if delete {
        app.send_rules.remove(idx);
        if app.send_rules.is_empty() {
            app.ui_state.selected_send_rule_idx = None;
        } else {
            app.ui_state.selected_send_rule_idx = Some(idx.min(app.send_rules.len() - 1));
        }
    }
}

/// ルールのモード状態を1行で要約 (リスト表示用)
fn summarize_mode(rule: &SendRule, t: &crate::i18n::Texts) -> String {
    match &rule.mode {
        SendMode::Manual => t.send_mode_manual.to_string(),
        SendMode::Interval { period_ms } => {
            format!(
                "{} {}{}",
                t.send_mode_interval, period_ms, t.send_interval_unit
            )
        }
        SendMode::OnReceive { pattern_text } => {
            if pattern_text.is_empty() {
                t.send_mode_on_receive.to_string()
            } else {
                format!("{} \"{}\"", t.send_mode_on_receive, pattern_text)
            }
        }
    }
}
