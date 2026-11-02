//! The DSL forms of the Rails routing guide, checked against what
//! `bin/rails routes` prints for them.

use super::*;
use std::fs;
use std::path::PathBuf;

fn fixture_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/rails-app"))
}

/// A project whose `config/routes.rb` is `main`, plus `config/routes/<name>.rb` files.
fn project(main: &str, drawn: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("config/routes")).unwrap();
    fs::write(dir.path().join("config/routes.rb"), main).unwrap();
    for (name, text) in drawn {
        fs::write(dir.path().join(format!("config/routes/{name}.rb")), text).unwrap();
    }
    dir
}

fn table(main: &str) -> RailsRouteTable {
    let dir = project(main, &[]);
    routes(dir.path()).expect("a Rails project")
}

/// `VERB /path controller#action [name]`, one per line, in table order.
fn rows(t: &RailsRouteTable) -> Vec<String> {
    t.routes
        .iter()
        .map(|r| {
            let verb = serde_json::to_value(r.verb).unwrap();
            let target =
                if r.action.is_empty() { r.controller.clone() } else { format!("{}#{}", r.controller, r.action) };
            let mut s = format!("{} {} {target}", verb.as_str().unwrap(), r.path);
            if let Some(n) = &r.name {
                s.push_str(&format!(" [{n}]"));
            }
            s
        })
        .collect()
}

fn assert_rows(src: &str, expected: &[&str]) {
    let t = table(src);
    assert_eq!(rows(&t), expected, "routes of:\n{src}\ndiagnostics: {:?}", t.diagnostics);
}

#[test]
fn not_a_rails_project() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!is_rails_project(dir.path()));
    assert!(routes(dir.path()).is_none());
}

#[test]
fn root_and_plain_routes() {
    let t = table("root to: \"pages#main\"\nroot \"welcome#index\"");
    assert_eq!(rows(&t), ["GET / pages#main [root]", "GET / welcome#index"]);
    assert_eq!(t.diagnostics[0].message, "Invalid route name, already in use: 'root'");
    assert_rows(
        r#"Rails.application.routes.draw do
  root to: "pages#main"
  get "/patients/:id", to: "patients#show", as: "patient"
  get "profile", to: "users#show"
  get "photos/show"
  get "photos/:id", to: "photos#show", as: nil
  match "photos", to: "photos#show", via: [:get, :post]
  match "legacy/*path" => "redirects#legacy", via: :all
  match "about" => :about, controller: "pages", via: :get
  get "/stories", to: redirect("/articles")
  post "exit", to: "sessions#destroy", as: :logout
  get "items", controller: "items", action: "index"
  get "sign-in", to: "sessions#new"
end"#,
        &[
            "GET / pages#main [root]",
            "GET /patients/:id patients#show [patient]",
            "GET /profile users#show [profile]",
            "GET /photos/show photos#show [photos_show]",
            "GET /photos/:id photos#show",
            "GET /photos photos#show [photos]",
            "POST /photos photos#show [photos]",
            "ANY /legacy/*path redirects#legacy",
            "GET /about pages#about [about]",
            "GET /stories redirect(/articles) [stories]",
            "POST /exit sessions#destroy [logout]",
            "GET /items items#index [items]",
            "GET /sign-in sessions#new [sign_in]",
        ],
    );
}

#[test]
fn resources_canonical_routes() {
    assert_rows(
        "resources :photos",
        &[
            "GET /photos photos#index [photos]",
            "POST /photos photos#create",
            "GET /photos/new photos#new [new_photo]",
            "GET /photos/:id/edit photos#edit [edit_photo]",
            "GET /photos/:id photos#show [photo]",
            "PATCH /photos/:id photos#update",
            "PUT /photos/:id photos#update",
            "DELETE /photos/:id photos#destroy",
        ],
    );
    let t = table("resources :photos");
    assert!(t.routes.iter().all(|r| r.source == RailsRouteSource::Resources && r.line == 1));
}

#[test]
fn singular_resource() {
    assert_rows(
        "resource :geocoder",
        &[
            "GET /geocoder/new geocoders#new [new_geocoder]",
            "GET /geocoder/edit geocoders#edit [edit_geocoder]",
            "GET /geocoder geocoders#show [geocoder]",
            "PATCH /geocoder geocoders#update",
            "PUT /geocoder geocoders#update",
            "DELETE /geocoder geocoders#destroy",
            "POST /geocoder geocoders#create",
        ],
    );
}

#[test]
fn resources_options() {
    assert_rows(
        r#"resources :photos, only: [:index, :show]
resources :books, except: [:destroy, :edit, :update]
resources :articles, path: "posts", as: "stories", controller: "blog/articles", param: :slug, only: [:index, :show]
resources :users, path_names: { new: "sign_up", edit: "profile" }, only: [:new, :edit]
resources :news, only: :index
resources :posts, module: "admin", only: :index
resources :items, :tags, only: :index"#,
        &[
            "GET /photos photos#index [photos]",
            "GET /photos/:id photos#show [photo]",
            "GET /books books#index [books]",
            "POST /books books#create",
            "GET /books/new books#new [new_book]",
            "GET /books/:id books#show [book]",
            "GET /posts blog/articles#index [stories]",
            "GET /posts/:slug blog/articles#show [story]",
            "GET /users/sign_up users#new [new_user]",
            "GET /users/:id/profile users#edit [edit_user]",
            "GET /news news#index [news_index]",
            "GET /posts admin/posts#index [posts]",
            "GET /items items#index [items]",
            "GET /tags tags#index [tags]",
        ],
    );
}

#[test]
fn nested_resources_and_member_collection_routes() {
    assert_rows(
        r#"resources :photos, only: [] do
  resources :comments, only: [:index, :show]
  member do
    get :preview
    post "publish"
  end
  collection do
    get :search
  end
  get :duplicate, on: :member
  get :compare, on: :collection
  get :preview, on: :new
  get :untouched
end
resources :magazines, only: [] do
  resources :ads, only: :index
end
resource :account, only: [] do
  resources :posts, only: :index
  get :history
end"#,
        &[
            "GET /photos/:photo_id/comments comments#index [photo_comments]",
            "GET /photos/:photo_id/comments/:id comments#show [photo_comment]",
            "GET /photos/:id/preview photos#preview [preview_photo]",
            "POST /photos/:id/publish photos#publish [publish_photo]",
            "GET /photos/search photos#search [search_photos]",
            "GET /photos/:id/duplicate photos#duplicate [duplicate_photo]",
            "GET /photos/compare photos#compare [compare_photos]",
            "GET /photos/new/preview photos#preview [preview_new_photo]",
            "GET /photos/:photo_id/untouched photos#untouched [photo_untouched]",
            "GET /magazines/:magazine_id/ads ads#index [magazine_ads]",
            "GET /account/posts posts#index [account_posts]",
            "GET /account/history accounts#history [history_account]",
        ],
    );
}

#[test]
fn shallow_nesting() {
    assert_rows(
        r#"resources :posts, shallow: true, only: [] do
  resources :comments, only: [:index, :create, :new, :show, :update]
end
shallow do
  resources :articles, only: [] do
    resources :notes, only: [:index, :show] do
      resources :likes, only: :index
    end
  end
end
namespace :admin do
  resources :photos, shallow: true, only: [] do
    resources :tags, only: [:index, :show]
  end
end"#,
        &[
            "GET /posts/:post_id/comments comments#index [post_comments]",
            "POST /posts/:post_id/comments comments#create",
            "GET /posts/:post_id/comments/new comments#new [new_post_comment]",
            "GET /comments/:id comments#show [comment]",
            "PATCH /comments/:id comments#update",
            "PUT /comments/:id comments#update",
            // Nested blocks run before the parent's canonical routes, as in `rails routes`.
            "GET /notes/:note_id/likes likes#index [note_likes]",
            "GET /articles/:article_id/notes notes#index [article_notes]",
            "GET /notes/:id notes#show [note]",
            "GET /admin/photos/:photo_id/tags admin/tags#index [admin_photo_tags]",
            "GET /admin/tags/:id admin/tags#show [admin_tag]",
        ],
    );
}

#[test]
fn namespace_scope_controller_constraints() {
    assert_rows(
        r#"namespace :admin do
  resources :articles, only: :index
  root to: "dashboard#index"
  get "stats", to: "stats#index"
end
namespace :api, path: "v1", as: "v1" do
  resources :photos, only: :index
end
scope module: "admin" do
  resources :reports, only: :index
end
scope "/admin" do
  resources :audits, only: :index
end
scope "/admin", as: "sekret" do
  resources :secrets, only: :index
end
scope path: "/x", module: "y", as: "z" do
  get "w", to: "a#b"
  get "abs", to: "/top#level"
end
controller :pages do
  get "about"
  get :terms
end
constraints subdomain: "api" do
  get "status", to: "api#status"
end
defaults format: :json do
  get "ping", to: "health#ping"
end"#,
        &[
            "GET /admin/articles admin/articles#index [admin_articles]",
            "GET /admin admin/dashboard#index [admin_root]",
            "GET /admin/stats admin/stats#index [admin_stats]",
            "GET /v1/photos api/photos#index [v1_photos]",
            "GET /reports admin/reports#index [reports]",
            "GET /admin/audits audits#index [audits]",
            "GET /admin/secrets secrets#index [sekret_secrets]",
            "GET /x/w y/a#b [z_w]",
            "GET /x/abs top#level [z_abs]",
            "GET /about pages#about [about]",
            "GET /terms pages#terms [terms]",
            "GET /status api#status [status]",
            "GET /ping health#ping [ping]",
        ],
    );
}

#[test]
fn concerns_in_main_and_drawn_files() {
    let dir = project(
        r#"draw :shared
concern :commentable do
  resources :comments, only: :index
end
resources :messages, only: [], concerns: :commentable
resources :articles, only: [] do
  concerns :commentable, :image_attachable
end
resources :posts, only: [], concerns: [:commentable]
concerns :commentable
concerns :missing
concern :ratable, Ratable
resources :videos, only: [], concerns: :ratable
"#,
        &[(
            "shared",
            r#"concern :image_attachable do
  resources :images, only: :index
end
"#,
        )],
    );
    let t = routes(dir.path()).unwrap();
    assert_eq!(
        rows(&t),
        [
            "GET /messages/:message_id/comments comments#index [message_comments]",
            "GET /articles/:article_id/comments comments#index [article_comments]",
            "GET /articles/:article_id/images images#index [article_images]",
            "GET /posts/:post_id/comments comments#index [post_comments]",
            "GET /comments comments#index [comments]",
        ]
    );
    assert!(t.routes.iter().all(|r| r.source == RailsRouteSource::Concern));
    // The concern's routes point at the concern definition, in the file holding it.
    let image = t.routes.iter().find(|r| r.path.contains("images")).unwrap();
    assert!(image.file.ends_with("config/routes/shared.rb"), "{:?}", image.file);
    assert_eq!(image.line, 2);
    let messages: Vec<String> = t.diagnostics.iter().map(|d| format!("{}: {}", d.line, d.message)).collect();
    assert_eq!(
        messages,
        [
            "12: concern :ratable defined by Ratable is not read statically; its routes are missing",
            "11: No concern named :missing was found",
        ]
    );
    assert_eq!(t.files.len(), 2);
}

#[test]
fn draw_inlines_files_in_the_current_scope() {
    let dir = project(
        r#"namespace :admin do
  draw :admin
end
draw(:api)
draw :nope
"#,
        &[("admin", "resources :photos, only: :index\n"), ("api", "scope \"/api\" do\n  draw :admin\nend\n")],
    );
    let t = routes(dir.path()).unwrap();
    assert_eq!(
        rows(&t),
        ["GET /admin/photos admin/photos#index [admin_photos]", "GET /api/photos photos#index [photos]"]
    );
    assert_eq!(t.diagnostics.len(), 1);
    assert_eq!(t.diagnostics[0].message, "`draw :nope`: config/routes/nope.rb not found");
    assert_eq!(t.diagnostics[0].line, 5);
    assert!(t.files.iter().any(|f| f.ends_with("config/routes/api.rb")));
    assert!(t.routes[0].file.ends_with("config/routes/admin.rb"));
}

#[test]
fn mount_direct_resolve_and_unreadable_calls() {
    let t = table(
        r##"mount Sidekiq::Web => "/sidekiq"
mount Blorgh::Engine, at: "/blog", as: "blorgh"
direct(:homepage) { "https://rubyonrails.org" }
resolve("Basket") { [:basket] }
get "#{ENV["PREFIX"]}/dynamic", to: "dynamic#show"
get path_var, to: "a#b"
match "no_via", to: "a#b"
get "lambda", to: ->(env) { [200, {}, []] }
resources :photos, only: :index, as: dynamic_name
get "ok", to: "fine#ok"
"##,
    );
    assert_eq!(
        rows(&t),
        ["ANY /sidekiq Sidekiq::Web [sidekiq_web]", "ANY /blog Blorgh::Engine [blorgh]", "GET /ok fine#ok [ok]"]
    );
    let lines: Vec<u32> = t.diagnostics.iter().map(|d| d.line).collect();
    assert_eq!(lines, [3, 4, 5, 6, 7, 8, 9]);
    assert!(t.diagnostics[2].message.contains("interpolated string"));
}

#[test]
fn fixture_app() {
    let t = routes(&fixture_root()).expect("fixtures/rails-app is a Rails project");
    let r = rows(&t);
    for expected in [
        "GET / pages#main [root]",
        "GET /photos/:photo_id/comments comments#index [photo_comments]",
        "GET /photos/new/preview photos#preview [preview_new_photo]",
        "GET /posts/:slug blog/articles#show [story]",
        "GET /comments/:id comments#show [comment]",
        "GET /messages/:message_id/comments/:id comments#show [message_comment]",
        "GET /articles/:article_id/images images#index [article_images]",
        "GET /admin admin/dashboard#index [admin_root]",
        "GET /admin/photos admin/photos#index [admin_photos]",
        "GET /admin/stats admin/stats#index [admin_stats]",
        "GET /admin/secrets secrets#index [sekret_secrets]",
        "ANY /sidekiq Sidekiq::Web [sidekiq_web]",
        "ANY /blog Blorgh::Engine [blorgh]",
    ] {
        assert!(r.contains(&expected.to_owned()), "missing {expected} in\n{}", r.join("\n"));
    }
    assert_eq!(t.files.len(), 3, "{:?}", t.files);
    let messages: Vec<&str> = t.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(messages.len(), 4, "{messages:?}");
    assert!(messages.iter().any(|m| m.contains("ratable")));
    assert!(messages.iter().any(|m| m.contains("missing.rb not found")));
    assert!(messages.iter().any(|m| m.starts_with("`direct`")));
    assert!(messages.iter().any(|m| m.contains("interpolated string")));
}

#[test]
fn with_options_and_conditionals() {
    assert_rows(
        r#"with_options via: :all do
  match "/404", to: "errors#not_found"
  match "/500", to: "errors#internal_server_error", via: :get
end
with_options only: :index do
  resources :photos
  resources :books, only: [:show]
end
if Rails.env.development?
  mount Lookbook::Engine, at: "/lookbook"
end
unless Rails.env.production?
  get "debug", to: "debug#index"
else
  get "status", to: "status#index"
end
get "flag", to: "flags#show" if ENV["FLAGS"]
case Rails.env
when "production" then get "prod", to: "env#prod"
else
  get "other", to: "env#other"
end"#,
        &[
            "ANY /404 errors#not_found",
            "GET /500 errors#internal_server_error",
            "GET /photos photos#index [photos]",
            "GET /books/:id books#show [book]",
            "ANY /lookbook Lookbook::Engine [lookbook_engine]",
            "GET /debug debug#index [debug]",
            "GET /status status#index [status]",
            "GET /flag flags#show [flag]",
            "GET /prod env#prod [prod]",
            "GET /other env#other [other]",
        ],
    );
}
