//! Shared helpers for the Git Graph's unit tests.

use warpui::elements::Element;

use crate::appearance::Appearance;

/// Renders `build` in a real window and lays it out, so a layout panic fails the test.
pub(crate) fn lay_out(build: fn(&Appearance) -> Box<dyn Element>) {
    use warpui::platform::WindowStyle;
    use warpui::{App, AppContext, Entity, SingletonEntity, TypedActionView, View, ViewContext};

    struct Host(fn(&Appearance) -> Box<dyn Element>);
    impl Entity for Host {
        type Event = ();
    }
    impl View for Host {
        fn ui_name() -> &'static str {
            "GitGraphColumnsTestHost"
        }
        fn render(&self, app: &AppContext) -> Box<dyn Element> {
            (self.0)(Appearance::as_ref(app))
        }
    }
    impl TypedActionView for Host {
        type Action = ();
        fn handle_action(&mut self, _: &(), _: &mut ViewContext<Self>) {}
    }

    App::test((), move |mut app| async move {
        app.add_singleton_model(|_| Appearance::mock());
        let (_, host) = app.add_window(WindowStyle::NotStealFocus, move |_| Host(build));
        host.update(&mut app, |_, ctx| ctx.notify());
    });
}
