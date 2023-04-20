use crate::auth::hash_password;
use chrono::NaiveDateTime;
use rocket::fairing::{self, AdHoc};
use rocket::serde::{json::Json, Deserialize, Serialize};
use rocket::{Build, Rocket};
use rocket_db_pools::{sqlx, sqlx::Row, Connection, Database};
use sqlx::sqlite::SqliteRow;
use std::collections::HashMap;

#[derive(Database, Debug, Clone)]
#[database("dev-db")]
pub struct Db(sqlx::SqlitePool);

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct User {
    #[serde(skip_deserializing, skip_serializing_if = "Option::is_none")]
    pub id: Option<u8>,
    pub email: String,
    pub name: String,
    pub password: String,
    pub created: String,
    pub profile_pic: String,
    pub admin: bool,
    pub premium: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct Admin {
    pub user: User,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct Project {
    #[serde(skip_deserializing, skip_serializing_if = "Option::is_none")]
    pub id: Option<u8>,
    pub name: String,
    pub proj_start_date: String,
    pub proj_end_date: String,
    pub owner: u8,
    pub participants: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct Projects(pub Vec<Project>);

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct ProjectTask {
    #[serde(skip_deserializing, skip_serializing_if = "Option::is_none")]
    pub id: Option<u8>,
    pub description: String,
    pub task_start_date: String,
    pub task_end_date: String,
    pub owner_proj: u8,
    pub time_delta: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct ProjectTasks(pub Vec<ProjectTask>);

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct ProjectWithTasks {
    pub project: Project,
    pub tasks: Option<ProjectTasks>,
}

fn serilaize_user(r: SqliteRow) -> Json<User> {
    Json(User {
        id: Some(r.get(0)),
        email: r.get(1),
        name: r.get(2),
        password: r.get(3),
        created: r.get(4),
        profile_pic: r.get(5),
        admin: r.get(6),
        premium: r.get(7),
    })
}

pub struct CompleteTask(pub ());

pub async fn get_user_by_id(mut db: Connection<Db>, id: u8) -> Option<Json<User>> {
    let result = sqlx::query(
        "SELECT id, email, name, password, created, profile_pic, admin, premium FROM user WHERE id = ?",
    )
    .bind(id)
    .fetch_one(&mut *db)
    .await;
    match result {
        Ok(r) => Some(serilaize_user(r)),
        Err(_) => None,
    }
}

pub async fn get_user_by_email(mut db: Connection<Db>, email: &str) -> Option<Json<User>> {
    let result = sqlx::query(
        "SELECT id, email, name, password, created, profile_pic, admin, premium FROM user WHERE email = ?",
    )
    .bind(email)
    .fetch_one(&mut *db)
    .await;
    match result {
        Ok(r) => Some(serilaize_user(r)),
        Err(_) => None,
    }
}

pub async fn user_req_guard(mut db: Connection<Db>, id: u8) -> Option<User> {
    let result = sqlx::query(
        "SELECT id, email, name, password, created, profile_pic, admin, premium FROM user WHERE id = ?",
    )
    .bind(id)
    .fetch_one(&mut *db)
    .await;
    match result {
        Ok(r) => Some(User {
            id: Some(r.get(0)),
            email: r.get(1),
            name: r.get(2),
            password: r.get(3),
            created: r.get(4),
            profile_pic: r.get(5),
            admin: r.get(6),
            premium: r.get(7),
        }),
        Err(_) => None,
    }
}

pub async fn get_all_projects_for_user(
    mut db: Connection<Db>,
    id: u8,
) -> Result<Vec<Project>, String> {
    let result = sqlx::query("SELECT * FROM project WHERE owner = ? ORDER BY proj_start_date DESC")
        .bind(id)
        .fetch_all(&mut *db)
        .await;
    match result {
        Ok(rows) => {
            let projects: Vec<Project> = rows
                .into_iter()
                .map(|row| {
                    Project {
                        id: row.get::<Option<u8>, _>("id"),
                        name: row.get("name"),
                        proj_start_date: row.get("proj_start_date"),
                        proj_end_date: row.get("proj_end_date"),
                        owner: row.get("owner"),
                        // Assuming participants is stored as a comma-separated string of u8 values
                        participants: row
                            .get::<String, _>("participants")
                            .split(',')
                            .filter_map(|s| s.parse::<u8>().ok())
                            .collect(),
                    }
                })
                .collect();
            Ok(projects)
        }
        Err(e) => Err(format!("Failed to get projects: {}", e)),
    }
}

pub async fn get_all_tasks_for_project(
    mut db: Connection<Db>,
    proj_id: u8,
) -> Result<Vec<ProjectTask>, String> {
    let result = sqlx::query("SELECT * FROM proj_tasks WHERE owner_proj = ?")
        .bind(proj_id)
        .fetch_all(&mut *db)
        .await;
    match result {
        Ok(rows) => {
            let tasks: Vec<ProjectTask> = rows
                .into_iter()
                .map(|row| ProjectTask {
                    id: row.get::<Option<u8>, _>("id"),
                    description: row.get("description"),
                    task_start_date: row.get("task_start_date"),
                    task_end_date: row.get("task_end_date"),
                    owner_proj: row.get("owner_proj"),
                    time_delta: row.get("time_delta"),
                })
                .collect();
            Ok(tasks)
        }
        Err(e) => Err(format!("Failed to get tasks: {}", e)),
    }
}

pub async fn get_all_projects_and_tasks_for_user(
    mut db: Connection<Db>,
    id: u8,
) -> Result<Vec<ProjectWithTasks>, String> {
    let result = sqlx::query(
        "
        SELECT p.*, t.id AS task_id, t.description, t.task_start_date, t.task_end_date, t.owner_proj, t.time_delta
    FROM project p
    LEFT JOIN (
        SELECT *,
        ROW_NUMBER() OVER (PARTITION BY owner_proj ORDER BY task_start_date DESC) AS row_num
        FROM proj_tasks
    ) t ON p.id = t.owner_proj AND t.row_num <= 3
    WHERE p.owner = ?
    ORDER BY p.proj_start_date DESC, t.task_start_date DESC",
    )
    .bind(id)
    .fetch_all(&mut *db)
    .await;
    match result {
        Ok(rows) => {
            let mut project_task_map: HashMap<u8, (Project, Vec<ProjectTask>)> = HashMap::new();

            for row in rows {
                let project_id = row.get::<u8, _>("id");
                let project = Project {
                    id: Some(project_id),
                    name: row.get("name"),
                    proj_start_date: row.get("proj_start_date"),
                    proj_end_date: row.get("proj_end_date"),
                    owner: row.get("owner"),
                    // Assuming participants is stored as a comma-separated string of u8 values
                    participants: row
                        .get::<String, _>("participants")
                        .split(',')
                        .filter_map(|s| s.parse::<u8>().ok())
                        .collect(),
                };

                let task_id: Option<u8> = row.get("task_id");
                if let Some(task_id) = task_id {
                    let task = ProjectTask {
                        id: Some(task_id),
                        description: row.get("description"),
                        task_start_date: row.get("task_start_date"),
                        task_end_date: row.get("task_end_date"),
                        owner_proj: row.get("owner_proj"),
                        time_delta: row.get("time_delta"),
                    };

                    let entry = project_task_map
                        .entry(project_id)
                        .or_insert((project, vec![]));
                    entry.1.push(task);
                } else {
                    project_task_map
                        .entry(project_id)
                        .or_insert((project, vec![]));
                }
            }

            let projects_with_tasks: Vec<ProjectWithTasks> = project_task_map
                .into_iter()
                .map(|(_, (project, tasks))| ProjectWithTasks {
                    project,
                    tasks: if tasks.is_empty() {
                        None
                    } else {
                        Some(ProjectTasks(tasks))
                    },
                })
                .collect();

            Ok(projects_with_tasks)
        }
        Err(e) => Err(format!("Failed to get projects: {}", e)),
    }
}

pub async fn get_project_by_id(mut db: Connection<Db>, id: u8) -> Result<Project, ()> {
    let result = sqlx::query("SELECT * FROM project WHERE id = ?")
        .bind(id)
        .fetch_one(&mut *db)
        .await;

    match result {
        Ok(row) => Ok({
            Project {
                id: row.get::<Option<u8>, _>("id"),
                name: row.get("name"),
                proj_start_date: row.get("proj_start_date"),
                proj_end_date: row.get("proj_end_date"),
                owner: row.get("owner"),
                // assuming participants is stored as a comma-separated string of u8 values
                participants: row
                    .get::<String, _>("participants")
                    .split(',')
                    .filter_map(|s| s.parse::<u8>().ok())
                    .collect(),
            }
        }),
        Err(e) => {
            error!("Failed to get project: {}", e);
            Err(())
        }
    }
}

pub async fn add_user(mut db: Connection<Db>, name: &str, email: &str, password: &str) {
    let password = hash_password(password);
    let result = sqlx::query!(
        "INSERT INTO user (name, email, password) VALUES (?, ?, ?)",
        name,
        email,
        password
    )
    .execute(&mut *db)
    .await;
    match result {
        Ok(_) => println!("User added successfully"),
        Err(e) => error!("Failed to add user: {}", e),
    }
}

pub async fn add_project(mut db: Connection<Db>, name: &str, id: u8) -> u8 {
    let result = sqlx::query!("INSERT INTO project (name, owner) VALUES (?, ?)", name, id,)
        .execute(&mut *db)
        .await;
    match &result {
        Ok(_) => println!("Project added successfully"),
        Err(e) => error!("Failed to add project: {}", e),
    }

    result.unwrap().last_insert_rowid() as u8
}

pub async fn add_task(mut db: Connection<Db>, description: &str, owner_proj: u8) -> u8 {
    let result = sqlx::query!(
        "INSERT INTO proj_tasks (description, owner_proj) VALUES (?, ?)",
        description,
        owner_proj,
    )
    .execute(&mut *db)
    .await;
    match &result {
        Ok(_) => println!("Task added successfully"),
        Err(e) => error!("Failed to add task: {}", e),
    }

    result.unwrap().last_insert_rowid() as u8
}

// edit_project(db, id, form_data.name, form_data.end_date)
pub async fn edit_project(mut db: Connection<Db>, id: u8, name: &str, proj_end_date: &str) -> u8 {
    // let proj_end_date = parse_date(proj_end_date);
    let result = sqlx::query!(
        "UPDATE project
        SET name = ?, proj_end_date = ?
        WHERE id = ?;",
        name,
        proj_end_date,
        id,
    )
    .execute(&mut *db)
    .await;
    match &result {
        Ok(_) => println!("Project edited successfully"),
        Err(e) => error!("Failed to edit project: {}", e),
    }
    // TODO: this is not returning the id of the edited project
    result.unwrap().last_insert_rowid() as u8
}

pub async fn delete_project_db(mut db: Connection<Db>, id: u8) -> Result<Option<()>, sqlx::Error> {
    let result = sqlx::query!("DELETE FROM project WHERE id = ?", id)
        .execute(&mut *db)
        .await?;

    Ok((result.rows_affected() == 1).then_some(()))
}

pub async fn delete_task_db(mut db: Connection<Db>, id: u8) -> Result<Option<()>, sqlx::Error> {
    let result = sqlx::query!("DELETE FROM proj_tasks WHERE id = ?", id)
        .execute(&mut *db)
        .await?;

    Ok((result.rows_affected() == 1).then_some(()))
}

pub async fn complete_task_db(mut db: Connection<Db>, id: u8) -> Result<Option<()>, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE proj_tasks SET task_end_date = strftime('%Y-%m-%dT%H:%M:%S', 'now') WHERE id = ?",
        id
    )
    .execute(&mut *db)
    .await?;

    Ok((result.rows_affected() == 1).then_some(()))
}

pub async fn add_time_delta(mut db: Connection<Db>, id: u8) -> Result<Option<()>, sqlx::Error> {
    let result = sqlx::query!(
        "
        UPDATE proj_tasks
        SET time_delta =
            CASE
                WHEN task_end_date IS NOT NULL THEN
                    datetime((strftime('%s', task_end_date) - strftime('%s', task_start_date)), 'unixepoch')
                ELSE
                    NULL
            END
        WHERE id = ?
        ",
        id
    )
    .execute(&mut *db)
    .await;

    match result {
        Ok(_) => {
            println!("Time delta added successfully");
            Ok(Some(()))
        }
        Err(e) => {
            error!("Failed to add time delta: {}", e);
            Err(e)
        }
    }
}

// parses from "2020-01-01T00:00:00" to "2020-01-01 00:00:00"
// "2020-01-01T00:00:00" is the format that the datepicker returns
// "2020-01-01 00:00:00" is the format generated by 'DATETIME DEFAULT CURRENT_TIMESTAMP' in sqlite
fn _parse_date(date: &str) -> String {
    let parsed_end_date = NaiveDateTime::parse_from_str(date, "%Y-%m-%dT%H:%M:%S")
        .expect("Failed to parse date string");
    parsed_end_date.format("%Y-%m-%d %H:%M:%S").to_string()
}

async fn run_migrations(rocket: Rocket<Build>) -> fairing::Result {
    match Db::fetch(&rocket) {
        Some(db) => match sqlx::migrate!("db/migrations").run(&**db).await {
            Ok(_) => Ok(rocket),
            Err(e) => {
                error!("Failed to initialize SQLx database: {}", e);
                Err(rocket)
            }
        },
        None => Err(rocket),
    }
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("user stage", |rocket| async {
        rocket
            .attach(Db::init())
            .attach(AdHoc::try_on_ignite("SQLx Migrations", run_migrations))
    })
}
