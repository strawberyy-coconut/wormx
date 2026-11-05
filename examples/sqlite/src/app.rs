use wormx::{Db, SqliteDialect};

wormx::table!("app_users");
wormx::table!("app_todos");

wormx::sql_function! {
    fn upper(value: String) -> String;
}

wormx::infix_operator! {
    fn concat(left: String, right: String) -> String = "||";
}

pub type ExampleDb = Db<SqliteDialect>;

pub async fn run(db: ExampleDb) -> Result<(), Box<dyn std::error::Error>> {
    let existing = db
        .query(AppUsers::all())
        .filter(AppUsers::fields().name().eq("Example User".to_owned()))
        .first()
        .await?;
    if let Some(user) = existing {
        let todos = db
            .query(AppTodos::all())
            .filter(AppTodos::fields().user_id().eq(user.id))
            .exec()
            .await?;
        for todo in todos {
            db.delete::<AppTodos>()
                .filter(AppTodos::fields().id().eq(todo.id))
                .exec()
                .await?;
        }
        db.delete::<AppUsers>()
            .filter(AppUsers::fields().id().eq(user.id))
            .exec()
            .await?;
    }

    println!("== users and their todos (nested eager loading) ==");
    let users = db
        .query(AppUsers::all())
        .order_by(AppUsers::fields().name().asc())
        .include(
            AppUsers::fields()
                .app_todos()
                .include(AppTodos::fields().user()),
        )
        .exec()
        .await?;

    for user in &users {
        println!(
            "{} <{}> active={}",
            user.name,
            user.email.as_deref().unwrap_or("-"),
            user.active
        );
        if let Some(todos) = user.app_todos().loaded() {
            for todo in todos {
                let owner = todo
                    .user()
                    .loaded()
                    .map(|owner| owner.name.as_str())
                    .unwrap_or("?");
                println!("  - {} (owner: {owner})", todo.title);
            }
        }
    }

    println!("== custom SQL extenders ==");
    let decorated: Vec<(String, String)> = db
        .query(AppUsers::all())
        .order_by(AppUsers::fields().id().asc())
        .select((
            upper(AppUsers::fields().name()),
            concat("user: ".to_owned(), AppUsers::fields().name()),
        ))
        .exec()
        .await?;
    for (name, label) in decorated {
        {
            println!("{name} -> {label}");
        }
    }

    println!("== insert / update / delete ==");

    db.insert::<AppUsers>()
        .value(AppUsers::fields().name(), "Example User".to_owned())
        .value(
            AppUsers::fields().email(),
            Some("example@wormx.dev".to_owned()),
        )
        .value(AppUsers::fields().active(), true)
        .exec()
        .await?;

    let user = db
        .query(AppUsers::all())
        .filter(AppUsers::fields().name().eq("Example User".to_owned()))
        .first()
        .await?
        .expect("inserted user");

    println!("== transaction ==");
    let mut transaction = db.begin().await?;
    db.insert::<AppTodos>()
        .value(AppTodos::fields().user_id(), user.id)
        .value(
            AppTodos::fields().title(),
            "Committed in a transaction".to_owned(),
        )
        .exec_in(&mut transaction)
        .await?;
    transaction.commit().await?;

    db.update::<AppUsers>()
        .set(AppUsers::fields().active(), false)
        .filter(AppUsers::fields().id().eq(user.id))
        .exec()
        .await?;

    let user = db
        .query(AppUsers::all())
        .filter(AppUsers::fields().id().eq(user.id))
        .first()
        .await?
        .expect("updated user");
    println!("{} is now active={}", user.name, user.active);

    let todo_count = db.query(AppTodos::all()).count().await?;
    println!("{todo_count} todos total");

    Ok(())
}
