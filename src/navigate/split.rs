pub struct Split {
    tree: Node,
    provider: Box<dyn Provider>,
    provider_name: String,
    view: View,
    cursor: (usize, IndexPath),
}
