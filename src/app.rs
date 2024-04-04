use crossterm::event::{self, Event, KeyCode, KeyEvent};
use std::{fmt::Display, io};

use crate::{
    postgres::connection_manager::ConnectionManager,
    widgets::{
        database::Database, database_cluster::DatabaseCluster, database_table::DatabaseTable,
    },
};

#[derive(Debug, PartialEq)]
pub enum InputMode {
    Normal,
    Editing,
}

pub struct PSQLConnectionOptions {
    pub host: String,
    pub user: String,
    pub dbname: Option<String>,
}

#[derive(PartialEq, Eq)]
pub enum FocusElement {
    Explorer,
    Main,
    SearchBar,
}

// App should store state which are separate from widgets.
// Widgets should read the state and determin what to render.
pub struct App {
    pub connection_manager: ConnectionManager,
    pub cluster: DatabaseCluster,
    pub debug_message: String,
    pub focused_element: FocusElement,
    pub input: String,
    pub input_mode: InputMode,
    pub show_debug: bool,
    pub show_keybinds: bool,
    pub should_quit: bool,
    pub title: String,
    input_history: Vec<String>,
}

impl App {
    pub fn new(title: String) -> App {
        let default_connection_options = PSQLConnectionOptions {
            user: String::from("dfeng"),
            host: String::from("localhost"),
            dbname: None,
        };

        let connection_manager_result = ConnectionManager::new(default_connection_options);

        let mut connection_manager = connection_manager_result.unwrap();

        let mut databases: Vec<Database> = connection_manager
            .get_databases()
            .into_iter()
            .map(|row| Database::new(row.get(0), Vec::new()))
            .collect();

        databases.sort_by(|a, b| a.name.cmp(&b.name));

        App {
            cluster: DatabaseCluster::new(databases),
            connection_manager,
            debug_message: String::from("test"),
            focused_element: FocusElement::Explorer,
            input: String::new(),
            input_mode: InputMode::Normal,
            input_history: Vec::new(),
            should_quit: false,
            show_debug: false,
            show_keybinds: true,
            title,
        }
    }

    // Register keybinds each time the app is updated.
    // Keybinds react to state and the current focused element
    //
    // Precedence order
    // 1) Input mode
    // 2) Focused Element
    //
    pub fn register_keybinds(&mut self) -> io::Result<()> {
        if let Event::Key(key) = event::read()? {
            match self.input_mode {
                InputMode::Normal => match key.code {
                    KeyCode::Char('1') => self.focused_element = FocusElement::Explorer,
                    KeyCode::Char('2') => self.focused_element = FocusElement::SearchBar,
                    KeyCode::Char('3') => self.focused_element = FocusElement::Main,
                    KeyCode::Char('q') => self.should_quit = true,
                    KeyCode::Char('b') => self.show_keybinds = !self.show_keybinds,
                    KeyCode::Char('d') => self.show_debug = !self.show_debug,
                    _ => match self.focused_element {
                        FocusElement::Main => self.register_main_keybinds(key),
                        FocusElement::Explorer => self.register_explorer_keybinds(key),
                        FocusElement::SearchBar => self.register_searchbar_keybinds(key),
                    },
                },
                InputMode::Editing => self.register_edit_mode_keybinds(key),
            }
        }

        Ok(())
    }

    fn register_edit_mode_keybinds(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.input_history.push(self.input.drain(..).collect());
            }
            KeyCode::Char(c) => {
                self.input.push(c);
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
            }
            _ => {}
        }
    }

    fn register_explorer_keybinds(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => self.select_database(),
            KeyCode::Char('j') => self.cluster.next(),
            KeyCode::Char('k') => self.cluster.prev(),
            KeyCode::Char('o') => self.open_table(),
            _ => {}
        }
    }

    fn register_main_keybinds(&mut self, key: KeyEvent) {
        match key.code {
            _ => {}
        }
    }

    fn register_searchbar_keybinds(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('i') => self.input_mode = InputMode::Editing,
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
            }
            _ => {}
        }
    }

    fn open_table(&mut self) {
        match self.cluster.select_focused_table().cloned() {
            Some(mut current_table) => {
                let columns = self
                    .connection_manager
                    .get_table(current_table.name.clone());

                match columns {
                    Ok(column_names_row) => {
                        let column_names: Vec<String> =
                            column_names_row.iter().map(|row| row.get(0)).collect();
                        current_table.set_columns(column_names);
                    }
                    Err(error) => self.show_debug_message(format!("Got an error: {}", error)),
                }

                let data = self.connection_manager.get_data(&current_table.name);

                match data {
                    Ok(data) => {
                        let data_as_text: Vec<String> = data.iter().map(|row| row.get(0)).collect();
                        current_table.set_data(data_as_text)
                    }

                    Err(error) => self.show_debug_message(format!("Got an error: {error}")),
                }
            }
            None => (),
        }
    }

    fn select_database(&mut self) {
        self.cluster.toggle_focused_database();

        for database in self.cluster.databases.iter_mut() {
            if database.is_connected {
                let database_name = database.name.clone();
                self.update_connection(&database_name);

                break;
            }
        }
    }

    fn update_connection(&mut self, database_name: &String) {
        let connection_options_for_databse = PSQLConnectionOptions {
            host: String::from("localhost"),
            user: String::from("dfeng"),
            dbname: Some(database_name.clone()),
        };

        let create_connection_result = self
            .connection_manager
            .create_database_connection(connection_options_for_databse);

        self.handle_error_with_debug(create_connection_result);
        let result = self.connection_manager.get_tables_for_database();
        let rows = self.handle_error_with_debug(result).unwrap_or_default();

        let mut table_names: Vec<String> = rows.iter().map(|row| row.get(0)).collect();
        table_names.sort();

        for database in self.cluster.databases.iter_mut() {
            if database.name == database_name.clone() {
                let tables_for_database = table_names
                    .into_iter()
                    .map(|name| DatabaseTable::new(name, Vec::new()))
                    .collect();

                database.tables = tables_for_database;

                break;
            }
        }
    }

    fn show_debug_message(&mut self, message: String) {
        self.debug_message = message;
        self.show_debug = true;
    }

    fn handle_error_with_debug<T, E: Display>(&mut self, result: Result<T, E>) -> Option<T> {
        match result {
            Ok(result) => Some(result),
            Err(error) => {
                let error_message = format!("Error encountered: {error}");
                self.debug_message = error_message;
                self.show_debug = true;
                None
            }
        }
    }
}
