# Loaded with `draw :concerns` before use in config/routes.rb.
concern :image_attachable do
  resources :images, only: :index
end
