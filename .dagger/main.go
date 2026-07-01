package main

import (
	"context"

	"dagger/otel-wasi/internal/dagger"
)

type OtelWasi struct {
	Source *dagger.Directory
}

func New(
	source *dagger.Workspace,
) *OtelWasi {
	return &OtelWasi{Source: source.Directory("/", dagger.WorkspaceDirectoryOpts{
		Exclude:   []string{".git"},
		Gitignore: true,
	})}
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
