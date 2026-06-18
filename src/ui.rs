//! All ratatui rendering lives here.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::{App, Mode};
use crate::collect::{short_home, Listener};

const ACCENT: Color = Color::Cyan;
const DEV: Color = Color::Green;

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Min(0),    // table
        Constraint::Length(1), // footer
    ])
    .split(f.area());

    header(f, chunks[0], app);
    table(f, chunks[1], app);
    footer(f, chunks[2], app);

    if app.mode == Mode::Confirm {
        confirm_popup(f, app);
    }
}

fn header(f: &mut Frame, area: Rect, app: &App) {
    let dev = app.rows.iter().filter(|l| l.is_dev).count();
    let sys = app.rows.len() - dev;
    let mut spans = vec![
        Span::styled("⚓ harbor", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(format!("{dev} dev"), Style::new().fg(DEV)),
        Span::styled(format!(" · {sys} system"), Style::new().dim()),
    ];
    if !app.show_system {
        spans.push(Span::styled("  [dev only]", Style::new().fg(ACCENT).dim()));
    }
    if !app.filter.is_empty() {
        spans.push(Span::styled(format!("  /{}", app.filter), Style::new().fg(Color::Yellow)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn table(f: &mut Frame, area: Rect, app: &mut App) {
    let header = Row::new([
        Cell::from("PORT"),
        Cell::from("NAME"),
        Cell::from("FRAMEWORK"),
        Cell::from("PID"),
        Cell::from("UPTIME"),
        Cell::from("CPU%"),
        Cell::from("PROJECT"),
    ])
    .style(Style::new().add_modifier(Modifier::BOLD).fg(ACCENT))
    .bottom_margin(0);

    let rows: Vec<Row> = app.rows.iter().map(row_for).collect();

    let widths = [
        Constraint::Length(11),
        Constraint::Length(16),
        Constraint::Length(11),
        Constraint::Length(7),
        Constraint::Length(12),
        Constraint::Length(5),
        Constraint::Min(18),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .column_spacing(1)
        .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▌");

    f.render_stateful_widget(table, area, &mut app.state);
}

fn row_for(l: &Listener) -> Row<'_> {
    let dot = if l.is_dev {
        Span::styled("● ", Style::new().fg(DEV))
    } else {
        Span::styled("· ", Style::new().dim())
    };
    let port = Span::styled(
        l.ports_str(),
        if l.is_dev {
            Style::new().fg(DEV).add_modifier(Modifier::BOLD)
        } else {
            Style::new().dim()
        },
    );
    let dim_if_sys = |s: String| {
        if l.is_dev {
            Span::raw(s)
        } else {
            Span::styled(s, Style::new().dim())
        }
    };

    let project = l
        .cwd
        .as_ref()
        .map(|p| short_home(p))
        .or_else(|| l.project.clone())
        .unwrap_or_default();

    Row::new(vec![
        Cell::from(Line::from(vec![dot, port])),
        Cell::from(dim_if_sys(l.command.clone())),
        Cell::from(Span::styled(
            l.framework.clone().unwrap_or_default(),
            Style::new().fg(Color::Magenta),
        )),
        Cell::from(dim_if_sys(l.pid.to_string())),
        Cell::from(dim_if_sys(l.uptime.clone())),
        Cell::from(dim_if_sys(l.cpu.clone())),
        Cell::from(dim_if_sys(project)),
    ])
}

fn footer(f: &mut Frame, area: Rect, app: &App) {
    let line = match app.mode {
        Mode::Filter => Line::from(vec![
            Span::styled(" filter ", Style::new().bg(Color::Yellow).fg(Color::Black)),
            Span::raw(" "),
            Span::raw(app.filter.clone()),
            Span::styled("▏", Style::new().fg(Color::Yellow)),
            Span::styled("   enter/esc to finish", Style::new().dim()),
        ]),
        Mode::Confirm => Line::from(Span::styled(
            "  confirm kill in the dialog…",
            Style::new().dim(),
        )),
        Mode::Normal => {
            if !app.status.is_empty() {
                Line::from(Span::styled(format!("  {}", app.status), Style::new().fg(DEV)))
            } else {
                help_line()
            }
        }
    };
    f.render_widget(Paragraph::new(line), area);
}

fn help_line() -> Line<'static> {
    let key = |k: &'static str| Span::styled(k, Style::new().fg(ACCENT).add_modifier(Modifier::BOLD));
    let lbl = |t: &'static str| Span::styled(t, Style::new().dim());
    Line::from(vec![
        Span::raw(" "),
        key("↑↓"), lbl(" move  "),
        key("x"), lbl(" kill  "),
        key("/"), lbl(" filter  "),
        key("a"), lbl(" all/dev  "),
        key("r"), lbl(" refresh  "),
        key("q"), lbl(" quit"),
    ])
}

fn confirm_popup(f: &mut Frame, app: &App) {
    let Some(l) = app.selected() else { return };
    let area = centered(60, 7, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Red))
        .title(Span::styled(" kill process ", Style::new().fg(Color::Red).add_modifier(Modifier::BOLD)));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let text = vec![
        Line::from(vec![
            Span::styled(l.command.clone(), Style::new().add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(format!(":{}", l.ports_str()), Style::new().fg(DEV)),
            Span::styled(format!("  pid {}", l.pid), Style::new().dim()),
        ]),
        Line::from(Span::styled(
            l.cwd.as_ref().map(|p| short_home(p)).unwrap_or_default(),
            Style::new().dim(),
        )),
        Line::raw(""),
        Line::from(vec![
            Span::styled("y", Style::new().fg(DEV).add_modifier(Modifier::BOLD)),
            Span::raw(" kill (SIGTERM)   "),
            Span::styled("K", Style::new().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw(" force (SIGKILL)   "),
            Span::styled("n/esc", Style::new().dim()),
            Span::styled(" cancel", Style::new().dim()),
        ]),
    ];
    f.render_widget(Paragraph::new(text), inner);
}

fn centered(w: u16, h: u16, area: Rect) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}
