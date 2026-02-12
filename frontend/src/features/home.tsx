// @ts-nocheck
import { navigate } from '../lib/navigation';

export function HomePage({ publicUrl, version }) {
    const installCommand = version ? `cargo install rise-deploy@${version}` : 'cargo install rise-deploy';

    return (
        <section>
            <div className="mono-docs-welcome">
                <header className="mono-docs-content-header">
                    <h3>Welcome to Rise</h3>
                </header>
                <p>Use the Docs section in the sidebar to browse guides and references.</p>
                <div className="mb-4">
                    <button
                        type="button"
                        className="mono-btn mono-btn-primary mono-btn-sm"
                        onClick={() => navigate('/docs')}
                    >
                        Open Docs
                    </button>
                </div>
                <div className="mono-docs-quickstart">
                    <div>
                        <h4># Install the Rise CLI and log in</h4>
                        <pre>{`$ ${installCommand}\n$ rise login --url ${publicUrl || window.location.origin}`}</pre>
                    </div>
                    <div>
                        <h4># Deploy a sample project</h4>
                        <pre>{`$ git clone https://github.com/GoogleCloudPlatform/buildpack-samples\n$ rise project create my-project\n$ rise deployment create my-project buildpack-samples/sample-python/`}</pre>
                    </div>
                </div>
            </div>
        </section>
    );
}
