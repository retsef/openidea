# The examples of https://guides.rubyonrails.org/routing.html, so every DSL
# form the Routes tool window understands has a route here.
Rails.application.routes.draw do
  root to: "pages#main"

  # Plain routes.
  get "/patients/:id", to: "patients#show", as: "patient"
  get "profile", to: "users#show"
  get "photos/show" # shorthand: photos#show
  match "photos", to: "photos#show", via: [:get, :post]
  match "legacy/*path" => "redirects#legacy", via: :all
  get "/stories", to: redirect("/articles")
  post "exit", to: "sessions#destroy", as: :logout

  # Resources, singular resource and options.
  resources :photos do
    resources :comments, only: [:index, :create]
    member do
      get :preview
    end
    collection do
      get :search
    end
    get :duplicate, on: :member
    get :compare, on: :collection
    get :preview, on: :new
  end
  resource :geocoder, except: :destroy
  resources :news
  resources :articles, path: "posts", as: "stories", controller: "blog/articles", param: :slug
  resources :users, path_names: { new: "sign_up", edit: "profile" }, only: [:new, :edit]

  # Shallow nesting.
  resources :posts, shallow: true do
    resources :comments
  end

  # Concerns, defined here and in config/routes/concerns.rb.
  draw :concerns
  concern :commentable do
    resources :comments
  end
  resources :messages, concerns: :commentable
  resources :articles, only: :index do
    concerns :commentable, :image_attachable
  end
  concern :ratable, Ratable # a class: not read statically

  # Namespaces and scopes.
  namespace :admin do
    resources :articles, only: [:index, :show]
    root to: "dashboard#index"
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
  controller :pages do
    get "about"
  end
  constraints subdomain: "api" do
    get "status", to: "api#status"
  end

  # Engines and drawn files.
  mount Sidekiq::Web => "/sidekiq"
  mount Blorgh::Engine, at: "/blog", as: "blorgh"
  draw :admin
  draw :missing # no such file: a diagnostic
  direct(:homepage) { "https://rubyonrails.org" } # not read statically

  # Not readable statically: a diagnostic, the rest of the table is kept.
  get "#{ENV["PREFIX"]}/dynamic", to: "dynamic#show"
end
