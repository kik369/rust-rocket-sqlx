#[macro_use]
extern crate rocket;

mod auth;
mod user;

use auth::verify_password;
use rocket::form::{Contextual, Form};
use rocket::fs::{relative, FileServer};
use rocket::http::{Cookie, CookieJar, Status};
use rocket::outcome::try_outcome;
use rocket::request::{FlashMessage, FromRequest, Outcome, Request};
use rocket::response::{Flash, Redirect};
use rocket_db_pools::Connection;
use rocket_dyn_templates::{context, Template};
use std::collections::HashMap;
use user::{
    add_project, add_task, add_user, delete_project_db, edit_project, get_all_projects_for_user,
    get_all_tasks_for_project, get_project_by_id, get_user_by_email, get_user_by_id,
    get_user_by_id_req_guard, Admin, Db, ProjectTasks, Projects, User,
};

// #[rocket::async_trait]
// impl<'r> FromRequest<'r> for User {
//     type Error = std::convert::Infallible;
//     async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
//         if let Some(cookie) = request.cookies().get_private("user_id_in_cookie") {
//             if let Ok(id) = cookie.value().parse::<u8>() {
//                 let db = request
//                     .guard::<Connection<Db>>()
//                     .await
//                     .succeeded()
//                     .expect("coul not establish db connection");
//                 match get_user_by_id_req_guard(db, id).await {
//                     Some(user) => {
//                         return Outcome::Success(user);
//                     }
//                     None => return Outcome::Forward(()),
//                 }
//             }
//         }
//         Outcome::Forward(())
//     }
// }

#[rocket::async_trait]
impl<'r> FromRequest<'r> for &'r User {
    type Error = std::convert::Infallible;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let user_result = request
            .local_cache_async(async {
                if let Some(cookie) = request.cookies().get_private("user_id_in_cookie") {
                    if let Ok(id) = cookie.value().parse::<u8>() {
                        let db = request
                            .guard::<Connection<Db>>()
                            .await
                            .succeeded()
                            .expect("could not establish db connection");
                        return get_user_by_id_req_guard(db, id).await;
                    }
                }
                None
            })
            .await;

        match user_result.as_ref() {
            Some(user) => Outcome::Success(user),
            None => Outcome::Forward(()),
        }
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Admin {
    type Error = std::convert::Infallible;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        // This will unconditionally query the database!
        let user = try_outcome!(request.guard::<&User>().await);
        if user.admin {
            {
                let user = user.clone();
                Outcome::Success(Admin { user })
            }
        } else {
            Outcome::Forward(())
        }
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Projects {
    type Error = std::convert::Infallible;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let user = try_outcome!(request.guard::<&User>().await);
        let db = request
            .guard::<Connection<Db>>()
            .await
            .succeeded()
            .expect("coul not establish db connection");
        // get all projects for the user
        let projects = get_all_projects_for_user(db, user.id.unwrap()).await;
        match projects {
            Ok(projects) => Outcome::Success(Projects(projects)),
            Err(_) => Outcome::Forward(()),
        }
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ProjectTasks {
    type Error = std::convert::Infallible;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        // this request guard will get all tasks for a project
        // project id comes from the url
        let proj_id = request.param(1).unwrap().unwrap();

        let db = request
            .guard::<Connection<Db>>()
            .await
            .succeeded()
            .expect("coul not establish db connection");
        // get all tasks for the project
        let tasks = get_all_tasks_for_project(db, proj_id).await;
        match tasks {
            Ok(tasks) => Outcome::Success(ProjectTasks(tasks)),
            Err(_) => Outcome::Forward(()),
        }
    }
}

// fn get_flash_msg(flash: Option<FlashMessage<'_>>) -> String {
//     flash
//         .map(|flash| format!("{}: {}", flash.kind(), flash.message()))
//         .unwrap_or_default()
// }

// fn get_flash_msg(flash: Option<FlashMessage<'_>>) -> String {
//     flash
//         .map(|flash| flash.message().to_string())
//         .unwrap_or_default()
// }

fn get_flash_msg(flash: Option<FlashMessage>) -> Result<(String, String), ()> {
    match flash {
        Some(flash) => Ok(flash.into_inner()),
        None => Err(()),
    }
}

#[get("/")]
fn index(user: &User) -> Template {
    Template::render("index", context! {user})
}

#[get("/", rank = 2)]
fn index_no_auth() -> Template {
    Template::render("index", context! {})
}

#[get("/login")]
fn login_get(user: &User) -> Template {
    Template::render("index", context! {user})
}

#[get("/login", rank = 2)]
fn login_get_no_auth() -> Template {
    Template::render("login", context! {})
}

#[get("/add-user")]
fn add_user_get(user: Option<&User>) -> Template {
    match user {
        Some(user) => Template::render("index", context! {user}),
        None => Template::render("add-user", context! {}),
    }
}

#[derive(FromForm, Debug)]
struct UserRegistrationForm<'v> {
    email: &'v str,
    name: &'v str,
    password: &'v str,
    password_check: &'v str,
}
#[post("/add-user", data = "<form>")]
async fn add_user_post<'r>(
    form: Form<Contextual<'r, UserRegistrationForm<'r>>>,
    db: Connection<Db>,
) -> (Status, Template) {
    let template = match form.value {
        Some(ref submission) => {
            if submission.password == submission.password_check {
                add_user(db, submission.name, submission.email, submission.password).await;
                Template::render("add-user", &form.context)
            } else {
                Template::render("add-user", context! {})
            }
        }
        None => Template::render("add-user", &form.context),
    };

    (form.context.status(), template)
}

#[derive(FromForm, Debug)]
struct LoginForm<'v> {
    email: &'v str,
    password: &'v str,
}

#[post("/login", data = "<form>")]
async fn login_post<'r>(
    cookies: &CookieJar<'_>,
    form: Form<Contextual<'r, LoginForm<'r>>>,
    db: Connection<Db>,
) -> Template {
    match form.value {
        Some(ref submission) => {
            // TODO: something with the to_string() and as_str() calls
            // TODO: also check query_user_by_email()
            let user = get_user_by_email(db, submission.email.to_string()).await;
            match user {
                Some(user) => {
                    if verify_password(submission.password, user.password.as_str()) {
                        cookies.add_private(Cookie::new(
                            "user_id_in_cookie",
                            user.id.expect("hmm").to_string(),
                        ));
                        let mut context = HashMap::new();
                        context.insert("user", user.0);
                        Template::render("index", context)
                    } else {
                        Template::render("login", context! {})
                    }
                }
                None => Template::render("login", context! {}),
            }
        }
        None => Template::render("login", context! {}),
    }
}

#[get("/logout")]
fn logout(cookies: &CookieJar<'_>) -> Template {
    cookies.remove_private(Cookie::named("user_id_in_cookie"));
    Template::render("index", context! {})
}

#[get("/user/<id>")]
async fn user_id(db: Connection<Db>, id: u8, admin: Admin) -> Template {
    let user = get_user_by_id(db, id).await;
    match user {
        Some(user) => Template::render(
            "user-id",
            context! {
                user: user.0,
                admin: admin.user
            },
        ),
        None => Template::render("index", context! {}),
    }
}

#[get("/user/<_id>", rank = 2)]
async fn user_id_no_auth(_id: u8) -> Redirect {
    Redirect::to(uri!("/"))
}

#[get("/profile")]
async fn profile(user: &User, projects: Projects, flash: Option<FlashMessage<'_>>) -> Template {
    let msg = get_flash_msg(flash);

    match msg {
        Ok(msg) => {
            let context = context! {user, projects, msg};
            Template::render("profile", &context)
        }
        Err(_) => Template::render("profile", context! {user, projects}),
    }
}

#[get("/project/<id>")]
async fn project_id(
    db: Connection<Db>,
    id: u8,
    user: &User,
    tasks: ProjectTasks,
) -> Result<Template, Redirect> {
    let project = get_project_by_id(db, id).await.unwrap();
    let context = context! {project, user, tasks};
    Ok(Template::render("project-id", &context))
}

#[derive(FromForm, Debug)]
struct EditProjectForm<'v> {
    name: &'v str,
    end_date: &'v str,
}

#[get("/edit/project/<id>")]
async fn edit_project_get(db: Connection<Db>, user: &User, id: u8) -> Result<Template, Redirect> {
    let project = get_project_by_id(db, id).await;

    match project {
        Ok(project) => {
            let context = context! {user, project};
            Ok(Template::render("project-edit", &context))
        }
        Err(_) => Err(Redirect::to(uri!("/"))),
    }
}

#[get("/edit/project/<_id>", rank = 2)]
async fn edit_project_get_no_auth(_id: u8) -> Redirect {
    Redirect::to(uri!("/login"))
}

#[post("/edit/project/<id>", data = "<form>")]
async fn edit_project_post<'r>(
    db: Connection<Db>,
    form: Form<Contextual<'r, EditProjectForm<'r>>>,
    user: Option<&User>,
    id: u8,
) -> Redirect {
    match user {
        Some(_user) => {
            let form_data = form.value.as_ref().unwrap();
            let id_too = id;
            let _id = edit_project(db, id, form_data.name, form_data.end_date).await;
            Redirect::to(uri!(project_id(id_too)))
        }
        None => Redirect::to(uri!("/login")),
    }
}

#[get("/delete/project/<id>")]
async fn delete_project(db: Connection<Db>, id: u8) -> Flash<Redirect> {
    let result = delete_project_db(db, id).await;
    match result {
        Ok(_) => Flash::success(Redirect::to(uri!("/profile")), "Project deleted"),
        Err(_) => Flash::error(Redirect::to(uri!("/profile")), "Hmm... That didn't work 🙃"),
    }
}

#[get("/add-project")]
fn add_project_get(user: Option<&User>) -> Result<Redirect, Template> {
    match user {
        Some(user) => {
            let context = context! {user};
            Err(Template::render("add-project", &context))
        }
        None => Ok(Redirect::to(uri!("/login"))),
    }
}

#[derive(FromForm, Debug)]
struct AddProjectForm<'v> {
    name: &'v str,
}

#[post("/add-project", data = "<form>")]
async fn add_project_post<'r>(
    db: Connection<Db>,
    form: Form<Contextual<'r, AddProjectForm<'r>>>,
    user: Option<&User>,
) -> Redirect {
    match user {
        Some(user) => {
            let form_data = form.value.as_ref().unwrap();
            let id = add_project(db, form_data.name, user.id.unwrap()).await;
            Redirect::to(uri!(project_id(id)))
        }
        None => Redirect::to(uri!("/login")),
    }
}

#[get("/project/<id>/add-task")]
async fn add_task_get(
    db: Connection<Db>,
    user: Option<&User>,
    id: u8,
) -> Result<Template, Redirect> {
    match user {
        Some(user) => {
            let project = get_project_by_id(db, id)
                .await
                .expect("should be a project");
            let context = context! {user, project};
            Ok(Template::render("add-task", &context))
        }
        None => Err(Redirect::to(uri!("/login"))),
    }
}

#[derive(FromForm, Debug)]
struct AddTaskForm<'v> {
    description: &'v str,
}

#[post("/project/<id>/add-task", data = "<form>")]
async fn add_task_post<'r>(
    db: Connection<Db>,
    form: Form<Contextual<'r, AddTaskForm<'r>>>,
    user: Option<&User>,
    id: u8,
) -> Redirect {
    match user {
        Some(_user) => {
            let form_data = form.value.as_ref().unwrap();
            let _task_id = add_task(db, form_data.description, id).await;
            Redirect::to(uri!(project_id(id)))
        }
        None => Redirect::to(uri!("/login")),
    }
}

#[launch]
fn rocket() -> _ {
    rocket::build()
        .attach(user::stage())
        .attach(Template::fairing())
        .mount(
            "/",
            routes![
                index,
                index_no_auth,
                login_get,
                login_get_no_auth,
                login_post,
                add_user_get,
                add_user_post,
                user_id,
                user_id_no_auth,
                profile,
                logout,
                add_project_get,
                add_project_post,
                project_id,
                edit_project_get,
                edit_project_get_no_auth,
                edit_project_post,
                delete_project,
                add_task_get,
                add_task_post,
            ],
        )
        .mount("/", FileServer::from(relative!("static/")))
}
