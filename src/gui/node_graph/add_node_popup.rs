/// A popup, where the user can choose a node he wants to add to the node graph.
pub struct AddNodePopup {
    categories: Vec<(&'static str, Item)>,
}

impl AddNodePopup {
    pub fn new(paths: &[&'static str]) -> Self {
        Self {
            categories: Self::find_categories(paths),
        }
    }

    fn find_categories(paths: &[&'static str]) -> Vec<(&'static str, Item)> {
        let paths = paths
            .iter()
            .map(|path| (path.split("/").collect::<Vec<_>>(), *path))
            .collect::<Vec<_>>();

        let mut root = Item::Category(Vec::new());

        for (path, full) in paths.iter() {
            root.add(&path, full);
        }

        match root {
            Item::Category(categories) => categories,
            Item::Node(_) => unreachable!(),
        }
    }

    pub fn show(&self, ui: &mut egui::Ui) -> Option<&str> {
        ui.label("Add Node");

        self.show_categories(ui, &self.categories)
    }

    fn show_categories(&self, ui: &mut egui::Ui, items: &[(&'static str, Item)]) -> Option<&str> {
        let mut result = None;
        for (name, item) in items {
            let res = match item {
                Item::Category(categories) => ui
                    .menu_button(*name, |ui| self.show_categories(ui, categories))
                    .inner
                    .unwrap_or_default(),
                Item::Node(path) => match ui.button(*name).clicked() {
                    true => Some(*path),
                    false => None,
                },
            };
            if let Some(res) = res {
                result = Some(res);
            }
        }
        result
    }
}

#[derive(Debug)]
enum Item {
    Category(Vec<(&'static str, Item)>),
    Node(&'static str),
}

impl Item {
    fn add(&mut self, path: &[&'static str], full_path: &'static str) {
        match self {
            Item::Category(categories) => {
                let Some(first) = path.first() else {
                    return;
                };

                let is_leaf = path.len() == 1;
                if is_leaf {
                    categories.push((*first, Item::Node(full_path)));
                } else {
                    if let Some((_, ref mut item)) =
                        categories.iter_mut().find(|(name, _)| name == first)
                    {
                        item.add(&path[1..], full_path);
                    } else {
                        let mut item = Item::Category(Vec::new());
                        item.add(&path[1..], full_path);
                        categories.push((*first, item));
                    }
                }
            }
            Item::Node(_) => {}
        }
    }
}
