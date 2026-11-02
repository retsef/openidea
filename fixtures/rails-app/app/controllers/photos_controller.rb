class PhotosController < ApplicationController
  def index
    @photos = Photo.all
  end

  def show
    @photo = Photo.find(params[:id])
  end

  # TODO: paginate the search results.
  def search
    @photos = Photo.where("title LIKE ?", "%#{params[:q]}%")
  end
end
