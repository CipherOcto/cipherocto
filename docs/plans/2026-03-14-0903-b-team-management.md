# Mission 0903-b: Team Management Implementation Plan

> **For Claude:** Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Minimal team management - expose what's already there, add basic team CRUD.

**Architecture:** Extend existing KeyStorage trait with team methods, add Team struct.

---

## Task 1: Add Team struct to models

**Files:** crates/quota-router-core/src/keys/models.rs

Add Team struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub team_id: String,
    pub name: String,
    pub budget_limit: i64,
    pub created_at: i64,
}
```

---

## Task 2: Add TeamStorage methods to storage.rs

**Files:** crates/quota-router-core/src/storage.rs

Add to KeyStorage trait or create separate trait:

```rust
fn create_team(&self, team: &Team) -> Result<(), KeyError>;
fn get_team(&self, team_id: &str) -> Result<Option<Team>, KeyError>;
fn list_teams(&self) -> Result<Vec<Team>, KeyError>;
fn delete_team(&self, team_id: &str) -> Result<(), KeyError>;
```

Implement in StoolapKeyStorage.

---

## Task 3: Tests

Add unit tests for team operations.

---

## Execution

Use subagent-driven development or implement directly.
