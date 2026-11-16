use warpui::App;

use super::NotesModel;

#[test]
fn changes_persist_and_reload() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("notes.json");
    App::test((), |mut app| async move {
        let model = app.add_model(|_| NotesModel::load_from(path.clone()));
        model.update(&mut app, |model, ctx| {
            model.add("ship notes", ctx);
            model.add("  ", ctx);
            let id = model.data().active().next().unwrap().id;
            model.toggle_done(id, ctx);
            model.archive_done(ctx);
        });

        let reloaded = NotesModel::load_from(path.clone());
        assert_eq!(reloaded.error(), None);
        assert_eq!(
            reloaded.data(),
            &model.read(&app, |model, _| model.data().clone())
        );
        assert_eq!(reloaded.data().archived_count(), 1);
    });
}

#[test]
fn unreadable_file_is_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.json");
    std::fs::write(&path, "not json").unwrap();
    App::test((), |mut app| async move {
        let model = app.add_model(|_| NotesModel::load_from(path.clone()));
        assert!(model.read(&app, |model, _| model.error()).is_some());
        model.update(&mut app, |model, ctx| model.add("a", ctx));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "not json");
    });
}
