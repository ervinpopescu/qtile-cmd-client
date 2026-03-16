use anyhow::Error;

pub(crate) static OBJECTS: &[&str] = &[
    "core", "screen", "bar", "widget", "group", "layout", "window", "root",
];

/// Represents a selector value — either a string name or a numeric index.
#[allow(dead_code)]
#[derive(Debug, Default)]
pub enum Selector {
    /// String for some objects
    String(String),
    /// Int for some other objects
    Int(u32),
    /// Select current object
    #[default]
    Null,
}

impl Clone for Selector {
    fn clone(&self) -> Self {
        match self {
            Selector::String(s) => Selector::String(s.clone()),
            Selector::Int(i) => Selector::Int(*i),
            Selector::Null => Selector::Null,
        }
    }
}

/// Qtile has a couple of object types which we save here for easy disambiguation.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum ObjectType {
    /// Screens are the display area that holds bars and an active group. Screen commands include changing the current group and changing the wallpaper.
    ///
    /// Screens can access objects displayed on that screen e.g. bar, widgets, groups, layouts and windows.
    ///
    /// Can have an index.
    Screen(Option<u32>),
    /// Groups are Qtile's workspaces. Groups are not responsible for the positioning of windows (that is handled by the [layouts](ObjectType::Layout)) so the available commands are somewhat more limited in scope.
    ///
    /// Groups have access to the layouts in that group, the windows in the group and the screen displaying the group.
    /// Can have an index.
    Group(Option<String>),
    /// Layouts position windows according to their specific rules. Layout commands typically include moving windows around the layout and changing the size of windows.
    ///
    /// Layouts can access the windows being displayed, the group holding the layout and the screen displaying the layout.
    ///
    /// Can have an index.
    Layout(Option<u32>),
    ///The size and position of windows is determined by the current layout. Nevertheless, windows can still change their appearance in multiple ways (toggling floating state, fullscreen, opacity).
    ///
    /// Windows can access objects relevant to the display of the window (i.e. the screen, group and layout).
    ///
    /// Can have an index
    Window(Option<u32>),
    /// The bar is primarily used to display widgets on the screen. As a result, the bar does not need many of its own commands.
    ///
    /// To select a bar on the command graph, you must use a selector (as there is no default bar). The selector is the position of the bar on the screen i.e. "top", "bottom", "left" or "right".
    ///
    /// The bar can access the screen it's on and the widgets it contains via the command graph.
    Bar(Option<String>),
    /// Widgets are small scripts that are used to provide content or add functionality to the bar. Some widgets will expose commands in order for functionality to be triggered indirectly (e.g. via a keypress).
    ///
    /// Widgets can access the parent bar and screen via the command graph.
    ///
    /// Has a required name
    Widget(Option<String>),
    /// The backend core is the link between the Qtile objects (windows, layouts, groups etc.) and the specific backend (X11 or Wayland). This core should be largely invisible to users and, as a result, these objects do not expose many commands.
    ///
    /// Nevertheless, both backends do contain important commands, notably set_keymap on X11 and change_vt used to change to a different TTY on Wayland.
    ///
    /// The backend core has no access to other nodes on the command graph.
    Core,
    /// The root node represents the main Qtile manager instance. Many of the commands on this node are therefore related to the running of the application itself.
    ///
    /// The root can access every other node in the command graph. Certain objects can be accessed without a selector resulting in the current object being selected (e.g. current group, screen, layout, window).
    Root,
}

/// Various implementations depending on the optional parameter of the object
impl ObjectType {
    /// For:
    /// - [`Bar`](ObjectType::Bar)
    /// - [`Widget`](ObjectType::Widget)
    pub fn with_string(string: &str, s: String) -> anyhow::Result<ObjectType> {
        match string {
            "bar" => Ok(Self::Bar(Some(s))),
            "group" => Ok(Self::Group(Some(s))),
            "widget" => Ok(Self::Widget(Some(s))),
            _ => Err(Error::msg(format!("Failed to parse {string}"))),
        }
    }
    /// For:
    /// - [`Screen`](ObjectType::Screen)
    /// - [`Group`](ObjectType::Group)
    /// - [`Layout`](ObjectType::Layout)
    /// - [`Window`](ObjectType::Window)
    pub fn with_number(string: &str, n: u32) -> anyhow::Result<ObjectType> {
        match string {
            "screen" => Ok(Self::Screen(Some(n))),
            "layout" => Ok(Self::Layout(Some(n))),
            "window" => Ok(Self::Window(Some(n))),
            _ => Err(Error::msg(format!("Failed to parse {string}"))),
        }
    }
    /// For:
    /// - [`Core`](ObjectType::Core)
    /// - [`Root`](ObjectType::Root)
    /// - [`None`]-initialized variants
    #[allow(dead_code)]
    pub fn with_none(string: &str) -> anyhow::Result<ObjectType> {
        match string {
            "screen" => Ok(Self::Screen(None)),
            "group" => Ok(Self::Group(None)),
            "layout" => Ok(Self::Layout(None)),
            "window" => Ok(Self::Window(None)),
            "bar" => Ok(Self::Bar(None)),
            "widget" => Ok(Self::Widget(None)),
            "core" => Ok(Self::Core),
            "root" => Ok(Self::Root),
            _ => Err(Error::msg(format!("Failed to parse {string}"))),
        }
    }
}
