package main

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"time"

	"dagger/otel-wasi/internal/dagger"
)

type OtelWasi struct {
	Source *dagger.Directory
}

func New(
	// +optional
	// +defaultPath="/"
	source *dagger.Directory,
) *OtelWasi {
	return &OtelWasi{Source: source}
}

// +check
func (m *OtelWasi) Build(ctx context.Context) error {
	_, err := m.rust().WithExec([]string{"cargo", "build"}).Sync(ctx)
	return err
}

// +check
func (m *OtelWasi) Test(ctx context.Context) error {
	_, err := m.rust().WithExec([]string{"cargo", "test", "--locked"}).Sync(ctx)
	return err
}

// +check
func (m *OtelWasi) BuildWasmcloudNatsEchoExample(ctx context.Context) error {
	_, err := m.wash().
		WithExec([]string{"rustup", "target", "add", "wasm32-wasip2"}).
		WithExec([]string{"wash", "-C", "examples/wasmcloud-nats-echo", "build", "--non-interactive"}).
		Sync(ctx)
	return err
}

func (m *OtelWasi) Publish(ctx context.Context, token *dagger.Secret) (string, error) {
	metadata, err := m.workspaceMetadata(ctx)
	if err != nil {
		return "", err
	}

	container := m.rust().WithSecretVariable("CARGO_REGISTRY_TOKEN", token)
	published := 0
	skipped := 0

	for _, pkg := range publishOrder(metadata) {
		if !pkg.publishable() {
			skipped++
			continue
		}

		exists, err := crateVersionExists(ctx, pkg.Name, pkg.Version)
		if err != nil {
			return "", err
		}
		if exists {
			skipped++
			continue
		}

		container = container.WithExec([]string{
			"cargo",
			"publish",
			"--locked",
			"--package",
			pkg.Name,
		})
		published++
	}

	_, err = container.Sync(ctx)
	if err != nil {
		return "", err
	}

	return fmt.Sprintf("published=%d skipped=%d", published, skipped), nil
}

func (m *OtelWasi) workspaceMetadata(ctx context.Context) (cargoMetadata, error) {
	out, err := m.rust().
		WithExec([]string{"cargo", "metadata", "--no-deps", "--format-version", "1"}).
		Stdout(ctx)
	if err != nil {
		return cargoMetadata{}, err
	}

	var metadata cargoMetadata
	if err := json.Unmarshal([]byte(out), &metadata); err != nil {
		return cargoMetadata{}, err
	}

	return metadata, nil
}

func (m *OtelWasi) rust() *dagger.Container {
	return dag.Container().
		From("rust:latest").
		WithEnvVariable("CARGO_HOME", "/cargo").
		WithMountedCache("/cargo/registry", dag.CacheVolume("cargo-registry")).
		WithMountedDirectory("/src", m.Source).
		WithMountedCache("/src/target", dag.CacheVolume("cargo-target")).
		WithWorkdir("/src")
}

func (m *OtelWasi) wash() *dagger.Container {
	rust := dag.Container().From("rust:latest")

	return dag.Container().
		From("ghcr.io/wasmcloud/wash:2.3.0").
		WithExec([]string{"apk", "add", "--no-cache", "build-base"}).
		WithDirectory("/usr/local/cargo", rust.Directory("/usr/local/cargo")).
		WithDirectory("/usr/local/rustup", rust.Directory("/usr/local/rustup")).
		WithEnvVariable("PATH", "/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin").
		WithEnvVariable("CARGO_HOME", "/cargo").
		WithEnvVariable("RUSTUP_HOME", "/usr/local/rustup").
		WithMountedCache("/cargo/registry", dag.CacheVolume("cargo-registry")).
		WithMountedDirectory("/src", m.Source).
		WithMountedCache("/src/target", dag.CacheVolume("cargo-target")).
		WithWorkdir("/src")
}

func publishOrder(metadata cargoMetadata) []cargoPackage {
	workspace := map[string]bool{}
	packages := map[string]cargoPackage{}
	idsByName := map[string]string{}

	for _, id := range metadata.WorkspaceMembers {
		workspace[id] = true
	}

	for _, pkg := range metadata.Packages {
		if !workspace[pkg.ID] {
			continue
		}
		packages[pkg.ID] = pkg
		idsByName[pkg.Name] = pkg.ID
	}

	visited := map[string]bool{}
	ordered := []cargoPackage{}

	var visit func(string)
	visit = func(id string) {
		if visited[id] {
			return
		}
		visited[id] = true

		pkg, ok := packages[id]
		if !ok {
			return
		}

		for _, dep := range pkg.Dependencies {
			depID, ok := idsByName[dep.Name]
			if ok && depID != id {
				visit(depID)
			}
		}

		ordered = append(ordered, pkg)
	}

	for _, id := range metadata.WorkspaceMembers {
		visit(id)
	}

	return ordered
}

func crateVersionExists(ctx context.Context, name string, version string) (bool, error) {
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, fmt.Sprintf("https://crates.io/api/v1/crates/%s/%s", name, version), nil)
	if err != nil {
		return false, err
	}
	request.Header.Set("User-Agent", "otel-wasi-dagger-ci")

	client := http.Client{Timeout: 30 * time.Second}
	response, err := client.Do(request)
	if err != nil {
		return false, err
	}
	defer response.Body.Close()

	if response.StatusCode == http.StatusOK {
		return true, nil
	}
	if response.StatusCode == http.StatusNotFound {
		return false, nil
	}

	return false, fmt.Errorf("crates.io lookup failed for %s %s with status %d", name, version, response.StatusCode)
}

type cargoMetadata struct {
	Packages         []cargoPackage `json:"packages"`
	WorkspaceMembers []string       `json:"workspace_members"`
}

type cargoPackage struct {
	ID           string            `json:"id"`
	Name         string            `json:"name"`
	Version      string            `json:"version"`
	Publish      any               `json:"publish"`
	Dependencies []cargoDependency `json:"dependencies"`
}

func (p cargoPackage) publishable() bool {
	registries, ok := p.Publish.([]any)
	return ok && len(registries) > 0
}

type cargoDependency struct {
	Name string `json:"name"`
}
