//! Tests for persistent child-state rendering and recursive handler routing.
//!
//! A parent that declares a field matching the lowercased component tag (e.g.
//! `pub todos: Arc<...>` for `<Todos>`) renders the child with its persistent
//! State, and the root event dispatcher routes child-template handlers to
//! `state.{owner}.{method}`.

use std::fs;

fn tmp_project_dir(label: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!("velox-sfc-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("create tmp dir");
    base
}

const APP_VX: &str = r#"<template>
  <div class="app">
    <Todos />
  </div>
</template>

<script setup>
import Todos from './components/Todos.vx'

pub struct State {
    pub todos: std::sync::Arc<super::todos::script_rs::State>,
}

impl State {
    pub fn new() -> Self {
        Self {
            todos: std::sync::Arc::new(super::todos::script_rs::State::new()),
        }
    }
    pub fn title(&self) -> String { String::from("Velox Todo") }
}
</script>
"#;

const TODOS_VX: &str = r#"<template>
  <div class="todos">
    <TodoItem
      v-for="(todo, idx) in todos"
      :todo="todo.text"
      :index="idx"
      @toggle="on_toggle"
      @remove="on_remove"
    />
  </div>
</template>

<script setup>
import TodoItem from './TodoItem.vx'
use velox_core::signal::Signal;

#[derive(Clone)]
pub struct Todo {
    pub text: String,
    pub completed: bool,
}

pub struct State {
    pub todos: Signal<Vec<Todo>>,
}

impl State {
    pub fn new() -> Self {
        Self { todos: Signal::new(vec![Todo { text: String::from("a"), completed: false }]) }
    }
    pub fn on_toggle(&self, _payload: &str) {}
    pub fn on_remove(&self, _payload: &str) {}
}
</script>
"#;

const TODO_ITEM_VX: &str = r#"<template>
  <div class="todo-item">
    <span>{{ text }}</span>
    <button @click="on_remove" :click-payload="index">×</button>
  </div>
</template>

<script setup>
pub struct Props { pub todo: String, pub index: String }
pub struct State { pub props: Props }
impl State {
    pub fn new() -> Self { Self { props: Props { todo: String::new(), index: String::new() } } }
    pub fn text(&self) -> String { self.props.todo.clone() }
    pub fn index(&self) -> String { self.props.index.clone() }
    pub fn on_remove(&self) {}
}
</script>
"#;

#[test]
fn persistent_child_state_rendering() {
    let dir = tmp_project_dir("persistent");
    fs::create_dir_all(dir.join("components")).expect("components dir");
    fs::write(dir.join("App.vx"), APP_VX).expect("App.vx");
    fs::write(dir.join("components/Todos.vx"), TODOS_VX).expect("Todos.vx");
    fs::write(dir.join("components/TodoItem.vx"), TODO_ITEM_VX).expect("TodoItem.vx");

    let sfc = velox_sfc::parse_sfc(APP_VX).expect("parse App.vx");
    let mut resolver = velox_sfc::ComponentResolver::new(dir.clone());
    resolver.parse_imports(&sfc.script_setup.as_ref().unwrap().content);
    let tpl = sfc.template.as_ref().unwrap().content.as_str();
    let script = sfc.script_setup.as_ref().map(|s| s.content.as_str());
    let rs = velox_sfc::compile_template_to_rs_full(tpl, "app", Some(&mut resolver), script, None)
        .expect("compile template");

    // The persistent `<Todos>` instance must render with its State, not a fresh
    // presentational State.
    assert!(
        rs.contains("Todos::render_with_state(std::sync::Arc::clone(&state.todos)"),
        "expected persistent child-state render, got:\n{rs}"
    );

    // The root dispatcher must route the child's handlers to state.todos.
    assert!(
        rs.contains("\"on_remove\" => { if let Some(p) = payload { state.todos.on_remove(p); } }")
            || rs.contains(
                "\"on_remove\" => { if let Some(p) = payload { state.todos.on_remove(p); }}"
            ),
        "expected child handler routed to state.todos.on_remove, got:\n{rs}"
    );
    assert!(
        rs.contains("\"on_toggle\"") && rs.contains("state.todos.on_toggle(p)"),
        "expected child handler routed to state.todos.on_toggle, got:\n{rs}"
    );

    // The v-for over the Signal must iterate the collection — verify against
    // the compiled Todos.vx module (the v-for lives there, not in App.vx).
    let todos_sfc = velox_sfc::parse_sfc(TODOS_VX).expect("parse Todos.vx");
    let mut tresolver = velox_sfc::ComponentResolver::new(dir.join("components"));
    tresolver.parse_imports(&todos_sfc.script_setup.as_ref().unwrap().content);
    let ttpl = todos_sfc.template.as_ref().unwrap().content.as_str();
    let tscript = todos_sfc.script_setup.as_ref().map(|s| s.content.as_str());
    let trs = velox_sfc::compile_template_to_rs_full(ttpl, "todos", Some(&mut tresolver), tscript, None)
        .expect("compile Todos template");
    assert!(
        trs.contains("let __col = state.todos.get();"),
        "expected State-mode v-for to read state.todos, got:\n{trs}"
    );
    assert!(
        trs.contains("for (idx, todo) in __col.iter().enumerate()"),
        "expected State-mode v-for enumerate, got:\n{trs}"
    );
}

#[test]
fn leaf_component_handlers_stay_local() {
    // A component that is NOT a persistent field keeps its handlers on its own
    // State (no `state.script_rs.{method}` leak).
    let dir = tmp_project_dir("leaf");
    fs::create_dir_all(dir.join("components")).expect("components dir");
    fs::write(dir.join("App.vx"), APP_VX).expect("App.vx");
    fs::write(dir.join("components/Todos.vx"), TODOS_VX).expect("Todos.vx");
    fs::write(dir.join("components/TodoItem.vx"), TODO_ITEM_VX).expect("TodoItem.vx");

    // Compile Todos.vx on its own: TodoItem is NOT a field of Todos.State, so
    // the handlers should route to Todos.State directly.
    let todos_sfc = velox_sfc::parse_sfc(TODOS_VX).expect("parse Todos.vx");
    let mut resolver = velox_sfc::ComponentResolver::new(dir.join("components"));
    resolver.parse_imports(&todos_sfc.script_setup.as_ref().unwrap().content);
    let tpl = todos_sfc.template.as_ref().unwrap().content.as_str();
    let script = todos_sfc.script_setup.as_ref().map(|s| s.content.as_str());
    let rs = velox_sfc::compile_template_to_rs_full(tpl, "todos", Some(&mut resolver), script, None)
        .expect("compile Todos template");

    assert!(
        rs.contains("state.on_remove(p)") && !rs.contains("state.script_rs.on_remove"),
        "expected local routing for leaf-owned handlers, got:\n{rs}"
    );
}
