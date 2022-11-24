use cursive::align::Align;
use cursive::direction::Orientation;
use cursive::theme::{BaseColor, Color, ColorStyle, Effect, Style};
use cursive::traits::{Boxable, Nameable};
use cursive::views::{Dialog, EditView, LinearLayout, ScrollView, TextView, ViewRef};
use cursive::Cursive;
use cursive_table_view::{TableView, TableViewItem};
use regex::Regex;
use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum RenameColumn {
    Original,
    Renamed,
}

// TODO: Show more of the path if it needs it to be unique
#[derive(Clone, Debug, PartialEq)]
struct RenameItem {
    original: String,
    renamed: String,
    file: PathBuf,
}

type RenameView = TableView<RenameItem, RenameColumn>;

impl RenameItem {
    fn new(path: PathBuf) -> Self {
        let original = path.file_name().unwrap().to_string_lossy();
        RenameItem {
            original: original.to_string(),
            renamed: original.to_string(),
            file: path,
        }
    }

    fn set_pattern(&mut self, find_pat: &Regex, replace_pat: &str) {
        self.renamed = find_pat
            .replace_all(&self.original, replace_pat)
            .to_string();
    }

    fn rename(&self) {
        let mut owned = self.file.to_owned();
        owned.push(&self.renamed)
    }
}

impl TableViewItem<RenameColumn> for RenameItem {
    fn to_column(&self, column: RenameColumn) -> String {
        match column {
            RenameColumn::Original => self.original.clone(),
            RenameColumn::Renamed => self.renamed.clone(),
        }
    }

    fn cmp(&self, other: &Self, column: RenameColumn) -> std::cmp::Ordering
    where
        Self: Sized,
    {
        match column {
            RenameColumn::Original => self.original.cmp(&other.original),
            RenameColumn::Renamed => self.renamed.cmp(&other.renamed),
        }
    }
}

struct RenamePatterns {
    // Not always synced with find_pat
    find_pat_raw: String,
    find_pat: Regex,
    replace_pat: String,
}

fn main() {
    cursive::logger::init();

    // Creates the cursive root - required for every application.
    let mut siv = cursive::default();

    let mut table = RenameView::new()
        .column(RenameColumn::Original, "Original", |c| c.width_percent(48))
        .column(RenameColumn::Renamed, "Renamed", |c| c.width_percent(48));

    let mut items = Vec::new();
    let mut failed_items = Vec::new();

    for filename in std::env::args().skip(1) {
        let path = PathBuf::from(filename);
        let string = path.to_string_lossy().to_string();
        if path.is_file() {
            items.push(RenameItem::new(path));
        } else if !path.exists() {
            failed_items.push(string);
        } else {
            log::debug!("Ignoring directory: {}", string);
        }
    }

    if items.is_empty() {
        // EARLY RETURN
        siv.add_layer(
            Dialog::text("No files provided!")
                .title("Error")
                .button("Close", |s| s.quit()),
        );
        siv.run();
        return;
    }

    table.set_items_stable(items);

    siv.set_user_data(RenamePatterns {
        find_pat_raw: "".to_string(),
        find_pat: Regex::new("").expect("Blank regex returns an error"),
        replace_pat: "".to_string(),
    });

    let mut error_style = Style::default();
    // let error_red = BaseColor::Red;

    error_style.color = ColorStyle::new(Color::Dark(BaseColor::Red), Color::Light(BaseColor::Blue));
    error_style.effects.insert(Effect::Underline);
    error_style.effects.insert(Effect::Bold);

    let main_layout = LinearLayout::new(Orientation::Vertical)
        .child(TextView::new("Find pattern:"))
        .child(
            EditView::new()
                .on_edit(on_edit_find_pattern)
                .on_submit(on_submit_find_pattern)
                .with_name("find_pattern"),
        )
        .child(TextView::new("Replace pattern:"))
        .child(
            EditView::new()
                .on_edit(on_edit_replace_pattern)
                .with_name("replace_pattern"),
        )
        .child(
            Dialog::around(table.with_name("file_table").min_size((50, 20)))
                .title("Files")
                .button("Cancel", |s| s.quit())
                .button("Settings", show_settings_window)
                .button("Apply", apply_renames),
        )
        .child(
            TextView::new("")
                .align(Align::bot_center())
                .style(error_style)
                .with_name("error_message"),
        )
        .full_screen();

    siv.add_layer(main_layout);

    if !failed_items.is_empty() {
        siv.add_layer(
            Dialog::around(ScrollView::new(
                LinearLayout::new(Orientation::Vertical).with_name("failed_items"),
            ))
            .title("Failed to access items: ")
            .button("Close", |s| {
                s.pop_layer();
            }),
        );

        siv.call_on_name("failed_items", |list: &mut LinearLayout| {
            for item in failed_items {
                list.add_child(TextView::new(item));
            }
        });
    }

    siv.add_global_callback('q', |s| s.quit());
    siv.add_global_callback('`', Cursive::toggle_debug_console);
    // Starts the event loop.
    siv.run();
}

fn show_settings_window(s: &mut Cursive) -> () {
    s.add_layer(
        Dialog::text("Settings not implemented")
            .dismiss_button("Close")
            .title("Settings"),
    )
}

fn on_edit_find_pattern(s: &mut Cursive, new_val: &str, _cursor: usize) {
    let mut patterns: &mut RenamePatterns = s.user_data().unwrap();
    patterns.find_pat_raw = new_val.to_string();

    match Regex::new(new_val) {
        Ok(re) => {
            patterns.find_pat = re;
            hide_error_message(s);
            update_renames(s);
        }
        // Simply do not change
        Err(err) => {
            let err = err.to_string();
            let short_err = err.lines().last().unwrap();
            set_error_message(s, short_err);
            log::warn!("{}", short_err);
        }
    }
}

/// Errors if there is a problem in the regex.
fn on_submit_find_pattern(s: &mut Cursive, new_val: &str) {
    let mut patterns: &mut RenamePatterns = s.user_data().unwrap();
    patterns.find_pat_raw = new_val.to_string();

    match Regex::new(new_val) {
        Ok(re) => {
            patterns.find_pat = re;
            update_renames(s);
        }
        Err(err) => {
            s.add_layer(
                Dialog::text(format!("{}", err))
                    .title("Pattern Error")
                    .button("Close", |s| {
                        s.pop_layer();
                    }),
            );
        }
    }
}

fn on_edit_replace_pattern(s: &mut Cursive, new_val: &str, _cursor: usize) {
    let mut patterns: &mut RenamePatterns = s.user_data().unwrap();
    patterns.replace_pat = new_val.to_string();
    update_renames(s);
}

fn update_renames(s: &mut Cursive) {
    let mut table: ViewRef<RenameView> = s.find_name("file_table").unwrap();
    let items = table.borrow_items_mut();
    let patterns: &RenamePatterns = s.user_data().unwrap();

    for item in items {
        item.set_pattern(&patterns.find_pat, &patterns.replace_pat);
    }
}

struct CheckResult {
    conflicting_names: Vec<String>,
    permission_problems: Vec<String>,
}

fn check_renames(items: &[RenameItem]) -> CheckResult {
    let mut unique_set = BTreeSet::<String>::new();
    let mut conflicting_names = Vec::new();

    let renamed_items = items.iter().map(|it| it.renamed.clone());
    for item in renamed_items {
        if !unique_set.insert(item.clone()) {
            // non unique
            conflicting_names.push(item.clone());
        }
    }

    let permission_problems = items
        .iter()
        .filter_map(|item| {
            match item.file.metadata() {
                Ok(meta) => meta.permissions().readonly(),
                Err(_) => true,
            }
            .then(|| item.file.to_string_lossy().to_string())
        })
        .collect();

    CheckResult {
        conflicting_names,
        permission_problems,
    }
}

fn apply_renames(s: &mut Cursive) {
    let mut table: ViewRef<RenameView> = s.find_name("file_table").unwrap();
    let items = table.borrow_items();
    let check_result = check_renames(items);

    let actual_length = items.len() - check_result.permission_problems.len();

    let do_rename = move |s: &mut Cursive, items: &Vec<RenameItem>| {
        for item in items {
            item.rename();
        }

        while s.pop_layer().is_some() {}

        s.add_layer(
            Dialog::text(format!("Renamed {} files ", actual_length))
                .button("Finish", |s| s.quit()),
        );
    };

    if check_result.conflicting_names.len() > 0 {
        let names_message = format!(
            "Files will be renamed to the same value:\n {}",
            check_result.conflicting_names.join(",\n ")
        );

        let items_clone = items.to_owned();

        let names_dialog = Dialog::text(names_message)
            .title("Conflicting names Error")
            .button("Cancel", |s| {
                s.pop_layer();
                s.call_on_name("perm_dialog", |v: &mut Dialog| {
                    v.buttons_mut().nth(1).unwrap().disable()
                });
            })
            .button("Continue", move |s| {
                s.pop_layer();
                // If the other warning has been dismissed, then do the operation
                if s.find_name::<Dialog>("perm_dialog").is_none() {
                    do_rename(s, &items_clone);
                }
            })
            .with_name("names_dialog");

        s.add_layer(names_dialog);
    }

    if check_result.permission_problems.len() > 0 {
        let perm_message = format!(
            "Files cannot be renamed:\n {}",
            check_result.permission_problems.join(",\n ")
        );

        let items_clone = items.to_owned();

        let perm_dialog = Dialog::text(perm_message)
            .title("Permissions Error")
            .button("Cancel", |s| {
                s.pop_layer();
                s.call_on_name("names_dialog", |v: &mut Dialog| {
                    v.buttons_mut().nth(1).unwrap().disable()
                });
            })
            .button("Continue", move |s| {
                s.pop_layer();
                // If the other warning has been dismissed, then do the operation
                if s.find_name::<Dialog>("names_dialog").is_none() {
                    do_rename(s, &items_clone);
                }
            })
            .with_name("perm_dialog");

        s.add_layer(perm_dialog);
    }
}

fn set_error_message(s: &mut Cursive, message: &str) {
    s.call_on_name("error_message", |v: &mut TextView| {
        v.set_content(message);
    });
}

fn hide_error_message(s: &mut Cursive) {
    set_error_message(s, "");
}
