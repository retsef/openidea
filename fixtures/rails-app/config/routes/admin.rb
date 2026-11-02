# Drawn into config/routes.rb with `draw :admin` (Rails 6.1+).
namespace :admin do
  resources :photos, only: [:index, :destroy]
  get "stats", to: "stats#index"
end
