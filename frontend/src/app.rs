use leptos::*;
use leptos_router::*;

use crate::pages::editor::EditorPage;
use crate::pages::projects::ProjectsPage;

#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <main>
                <Routes>
                    <Route path="/" view=ProjectsPage/>
                    <Route path="/projects/:id" view=EditorPage/>
                </Routes>
            </main>
        </Router>
    }
}
