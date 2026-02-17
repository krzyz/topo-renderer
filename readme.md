# Introduction

A simple hobby project to display vistas/panorama from any place of the world using open source data and WebGPU rendering (via rust).

# Live demo

See at [https://topo.realcomplexity.com](https://topo.realcomplexity.com)

![topo](https://github.com/user-attachments/assets/4a38478f-0e91-4512-8bfe-ad41ff0e538c)

# Development

To build/run, either install [just](https://just.systems/) and use commands from sections below or look up the actual commands from `justfile`.

## Settings

The project settings need to be set in `Settings.toml` file (best placed in the root directory) with the following settings defined:
- `data_dir` which specifies the location of peak (latitude, longitude, name, elevation (in meters) csv files) and DEM (COP 90 copernicus dataset) data, which are read and served by the backend
- `backend_url` which is the address of the backend that is used by the renderer (in order to fetch the peak/DEM data)

## Backend

`just backend`

## Running desktop version

`just desktop` or `just desktop-debug`

## Running wasm version

`just build-wasm`
`just serve-wasm`

*Important* WebGPU only works on HTTPS and localhost, so use "localhost:8080" instead of "0.0.0.0:8080" when running a local build in the browser!
