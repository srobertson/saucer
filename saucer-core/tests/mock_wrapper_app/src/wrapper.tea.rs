use saucer_core::Cmd;
use mock_widget::widget;
use std::rc::Rc;

pub struct Model<InnerModel> {
    pub inner: InnerModel,
}

#[derive(Clone, Debug)]
pub enum Msg<InnerMsg> {
    Inner(InnerMsg),
    Widget(widget::Msg),
}

pub struct AppFns<InnerModel, InnerMsg, InnerView> {
    pub init: fn() -> (InnerModel, Cmd<InnerMsg>),
    pub update: fn(InnerModel, InnerMsg) -> (InnerModel, Cmd<InnerMsg>),
    pub view: fn(&InnerModel) -> InnerView,
}

pub struct App<InnerModel, InnerMsg, InnerView> {
    pub init: Box<dyn Fn() -> (Model<InnerModel>, Cmd<Msg<InnerMsg>>) + 'static>,
    pub update: Box<dyn Fn(Model<InnerModel>, Msg<InnerMsg>) -> (Model<InnerModel>, Cmd<Msg<InnerMsg>>) + 'static>,
    pub view: Box<dyn Fn(&Model<InnerModel>) -> InnerView + 'static>,
}

pub fn make_app<InnerModel: 'static, InnerMsg: Clone + 'static, InnerView: 'static>(
    inner: AppFns<InnerModel, InnerMsg, InnerView>,
) -> App<InnerModel, InnerMsg, InnerView> {
    let inner = Rc::new(inner);
    let inner_for_init = inner.clone();
    let inner_for_update = inner.clone();
    let inner_for_view = inner.clone();
    fn init<InnerModel: 'static, InnerMsg: 'static, InnerView: 'static>(
        inner: &AppFns<InnerModel, InnerMsg, InnerView>,
    ) -> (Model<InnerModel>, Cmd<Msg<InnerMsg>>) {
        let (inner_model, inner_cmd) = (inner.init)();
        let (_w_model, widget_cmd) = widget::init();
        let model = Model { inner: inner_model };
        // Also kick the widget once so its command path is exercised.
        let trigger = widget::update(widget::Model {}, widget::Msg::Triggered).1.map(Msg::Widget);
        let cmd = Cmd::batch([inner_cmd.map(Msg::Inner), widget_cmd.map(Msg::Widget), trigger]);
        (model, cmd)
    }

    fn update<InnerModel: 'static, InnerMsg: 'static, InnerView: 'static>(
        inner: &AppFns<InnerModel, InnerMsg, InnerView>,
        model: Model<InnerModel>,
        msg: Msg<InnerMsg>,
    ) -> (Model<InnerModel>, Cmd<Msg<InnerMsg>>) {
        match msg {
            Msg::Inner(imsg) => {
                let (inner_model, inner_cmd) = (inner.update)(model.inner, imsg);
                (Model { inner: inner_model }, inner_cmd.map(Msg::Inner))
            }
            Msg::Widget(wmsg) => {
                let (_widget_model, widget_cmd) = widget::update(widget::Model {}, wmsg);
                (model, widget_cmd.map(Msg::Widget))
            }
        }
    }

    fn view<InnerModel, InnerMsg, InnerView: 'static>(
        inner: &AppFns<InnerModel, InnerMsg, InnerView>,
        model: &Model<InnerModel>,
    ) -> InnerView {
        (inner.view)(&model.inner)
    }

    App {
        init: Box::new(move || init(&inner_for_init)),
        update: Box::new(move |m, msg| update(&inner_for_update, m, msg)),
        view: Box::new(move |m| view(&inner_for_view, m)),
    }
}
