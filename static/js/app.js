// React-based Rise Dashboard Application with Tailwind CSS
// Main application router with Header, LoginPage, and App components
const { useState, useEffect } = React;
// CONFIG is already defined in auth.js which loads before this script

// Header Component
function Header({ user, onLogout, currentView, onShowGettingStarted }) {
    const [isProfileOpen, setIsProfileOpen] = useState(false);
    const profileRef = React.useRef(null);
    const { showToast } = useToast();

    // Determine which section is active (projects or teams)
    const isProjectsActive = currentView === 'projects' || currentView === 'project-detail' || currentView === 'deployment-detail' || currentView === 'extension-detail';
    const isTeamsActive = currentView === 'teams' || currentView === 'team-detail';

    // Close dropdown when clicking outside
    useEffect(() => {
        function handleClickOutside(event) {
            if (profileRef.current && !profileRef.current.contains(event.target)) {
                setIsProfileOpen(false);
            }
        }

        if (isProfileOpen) {
            document.addEventListener('mousedown', handleClickOutside);
            return () => document.removeEventListener('mousedown', handleClickOutside);
        }
    }, [isProfileOpen]);

    const handleCopyJWT = () => {
        const token = localStorage.getItem('rise_token');
        if (token) {
            navigator.clipboard.writeText(token).then(() => {
                showToast('JWT token copied to clipboard', 'success');
                setIsProfileOpen(false);
            }).catch(() => {
                showToast('Failed to copy JWT token', 'error');
            });
        }
    };

    return (
        <header className="bg-gray-900 border-b border-gray-800">
            <nav className="container mx-auto px-4 py-4">
                <div className="flex items-center justify-between">
                    <div className="flex items-center gap-4">
                        <a href="#projects" className="flex items-center gap-2 hover:opacity-80 transition-opacity">
                            <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                <path d="M12 2L2 7L12 12L22 7L12 2Z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                                <path d="M2 17L12 22L22 17" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                                <path d="M2 12L12 17L22 12" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                            </svg>
                            <strong className="text-lg font-bold">Rise Dashboard</strong>
                        </a>
                        <button
                            onClick={onShowGettingStarted}
                            className="flex items-center gap-2 px-3 py-1.5 text-sm font-medium text-white bg-indigo-600 hover:bg-indigo-700 rounded-md transition-colors"
                        >
                            <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                            </svg>
                            Getting Started
                        </button>
                    </div>
                    <div className="flex items-center gap-6">
                        <a
                            href="#projects"
                            className={`transition-colors ${isProjectsActive ? 'text-indigo-400 font-semibold' : 'text-gray-300 hover:text-white'}`}
                        >
                            Projects
                        </a>
                        <a
                            href="#teams"
                            className={`transition-colors ${isTeamsActive ? 'text-indigo-400 font-semibold' : 'text-gray-300 hover:text-white'}`}
                        >
                            Teams
                        </a>

                        {/* User Profile Dropdown */}
                        <div className="relative" ref={profileRef}>
                            <button
                                onClick={() => setIsProfileOpen(!isProfileOpen)}
                                className="flex items-center gap-2 hover:opacity-80 transition-opacity"
                            >
                                <div className="w-8 h-8 rounded-full bg-indigo-600 flex items-center justify-center border-2 border-indigo-500">
                                    <svg className="w-5 h-5 text-white" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                        <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                                        <circle cx="12" cy="7" r="4" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                                    </svg>
                                </div>
                            </button>

                            {isProfileOpen && (
                                <div className="absolute right-0 mt-2 w-64 bg-gray-800 border border-gray-700 rounded-lg shadow-xl z-50">
                                    <div className="p-4 border-b border-gray-700">
                                        <p className="text-sm text-gray-400 mb-1">Signed in as</p>
                                        <p className="text-white font-medium break-all">{user?.email}</p>
                                    </div>
                                    <div className="p-2">
                                        <button
                                            onClick={handleCopyJWT}
                                            className="w-full flex items-center gap-2 px-3 py-2 text-left text-gray-300 hover:bg-gray-700 rounded transition-colors"
                                        >
                                            <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                                <rect x="9" y="9" width="13" height="13" rx="2" ry="2" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                                                <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                                            </svg>
                                            Copy JWT Token
                                        </button>
                                        <button
                                            onClick={() => { setIsProfileOpen(false); onLogout(); }}
                                            className="w-full flex items-center gap-2 px-3 py-2 text-left text-red-400 hover:bg-gray-700 rounded transition-colors"
                                        >
                                            <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                                <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                                                <polyline points="16 17 21 12 16 7" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                                                <line x1="21" y1="12" x2="9" y2="12" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                                            </svg>
                                            Logout
                                        </button>
                                    </div>
                                </div>
                            )}
                        </div>
                    </div>
                </div>
            </nav>
        </header>
    );
}

// Getting Started Modal Component
function GettingStartedModal({ isOpen, onClose, publicUrl, version }) {
    const installCommand = version ? `cargo install rise-deploy@${version}` : 'cargo install rise-deploy';

    return (
        <Modal isOpen={isOpen} onClose={onClose} title="Getting Started" maxWidth="max-w-3xl">
            <div className="space-y-4 text-gray-300">
                <p>This is how you get started with your first Rise project:</p>

                <div className="bg-gray-800 rounded-lg p-4 space-y-4">
                    <div>
                        <h4 className="text-sm font-semibold text-gray-400 mb-2"># Install the Rise CLI and log-in</h4>
                        <pre className="text-sm text-indigo-300 overflow-x-auto whitespace-pre-wrap">$ {installCommand}{'\n'}$ rise login --url {publicUrl || window.location.origin}</pre>
                    </div>

                    <div>
                        <h4 className="text-sm font-semibold text-gray-400 mb-2"># Deploy a sample project</h4>
                        <pre className="text-sm text-indigo-300 overflow-x-auto whitespace-pre-wrap">$ git clone https://github.com/GoogleCloudPlatform/buildpack-samples{'\n'}$ rise project create my-project # Pick a unique project name{'\n'}$ rise deployment create my-project buildpack-samples/sample-python/</pre>
                    </div>
                </div>

                <p className="text-sm text-gray-400 mt-4">
                    For more information, visit the{' '}
                    <a
                        href="https://github.com/NiklasRosenstein/rise"
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-indigo-400 hover:text-indigo-300 underline"
                    >
                        Rise documentation on GitHub
                    </a>.
                </p>
            </div>
        </Modal>
    );
}

// Login Page Component
function LoginPage() {
    const [status, setStatus] = useState('');
    const [loading, setLoading] = useState(false);

    // Handle OAuth callback on component mount
    useEffect(() => {
        const params = new URLSearchParams(window.location.search);
        if (params.has('code')) {
            setStatus('Processing authentication...');
            setLoading(true);
            handleOAuthCallback()
                .catch((error) => {
                    setStatus(`Error: ${error.message}`);
                    setLoading(false);
                });
        }
    }, []);

    const handleLogin = async () => {
        setStatus('Initializing authentication...');
        setLoading(true);
        try {
            await login();
        } catch (error) {
            setStatus(`Error: ${error.message}`);
            setLoading(false);
        }
    };

    return (
        <div className="flex items-center justify-center min-h-screen bg-gradient-to-br from-gray-900 via-gray-950 to-black">
            <div className="w-full max-w-md p-8 bg-gray-900 rounded-lg border border-gray-800 shadow-2xl">
                <div className="text-center mb-8">
                    <div className="flex justify-center mb-4">
                        <svg className="w-16 h-16 text-indigo-500" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                            <path d="M12 2L2 7L12 12L22 7L12 2Z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                            <path d="M2 17L12 22L22 17" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                            <path d="M2 12L12 17L22 12" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                        </svg>
                    </div>
                    <h1 className="text-3xl font-bold text-white mb-2">Rise</h1>
                    <p className="text-gray-400">Container Deployment Platform</p>
                </div>

                {loading ? (
                    <div className="flex flex-col items-center gap-4 py-8">
                        <div className="w-12 h-12 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div>
                        <p className="text-gray-300">{status}</p>
                    </div>
                ) : (
                    <>
                        <button
                            onClick={handleLogin}
                            className="w-full bg-indigo-600 hover:bg-indigo-700 text-white font-semibold py-3 px-4 rounded-lg transition-colors mb-4"
                        >
                            Login with OAuth
                        </button>
                        {status && (
                            <p className="text-center text-sm text-red-400">{status}</p>
                        )}
                    </>
                )}
            </div>
        </div>
    );
}

// Main App Component
function App() {
    const [user, setUser] = useState(null);
    const [authChecked, setAuthChecked] = useState(false);
    const [showGettingStarted, setShowGettingStarted] = useState(false);
    const [version, setVersion] = useState(null);
    const hash = useHashLocation();

    useEffect(() => {
        // Check if we're handling OAuth callback
        const params = new URLSearchParams(window.location.search);
        if (params.has('code')) {
            // Let LoginPage handle the callback
            setAuthChecked(true);
            return;
        }

        if (!isAuthenticated()) {
            setAuthChecked(true);
            return;
        }

        async function loadUser() {
            try {
                const userData = await api.getMe();
                setUser(userData);
            } catch (err) {
                console.error('Failed to load user:', err);
                logout();
            } finally {
                setAuthChecked(true);
            }
        }
        loadUser();
    }, []);

    useEffect(() => {
        async function fetchVersion() {
            try {
                const response = await fetch(`${CONFIG.backendUrl}/api/v1/version`);
                const data = await response.json();
                setVersion(data);
            } catch (err) {
                console.error('Failed to fetch version:', err);
            }
        }
        fetchVersion();
    }, []);

    const handleLogout = () => {
        logout();
    };

    if (!authChecked) {
        return (
            <div className="flex items-center justify-center min-h-screen">
                <div className="w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div>
            </div>
        );
    }

    if (!isAuthenticated() || !user) {
        return <LoginPage />;
    }

    // Parse hash for routing
    let view = 'projects';
    let params = {};

    if (hash.startsWith('project/')) {
        const parts = hash.split('/');
        // Check if this is an extension detail page
        // project/{name}/extensions - extensions tab (list view)
        // project/{name}/extensions/{type}/@new - creating new instance
        // project/{name}/extensions/{type}/{instance} - viewing existing instance
        if (parts[2] === 'extensions') {
            if (parts.length === 3) {
                // Just the extensions tab: project/{name}/extensions
                view = 'project-detail';
                params.projectName = parts[1];
                params.tab = 'extensions';
            } else if (parts.length === 4 && parts[3] === '@new') {
                // Creating new instance: project/{name}/extensions/@new
                view = 'extension-create';
                params.projectName = parts[1];
                params.extensionType = null; // Will show list to choose from
            } else if (parts.length === 5 && parts[3]) {
                // Creating new instance of specific type: project/{name}/extensions/{type}/@new
                // or viewing instance: project/{name}/extensions/{type}/{instance}
                if (parts[4] === '@new') {
                    view = 'extension-detail';
                    params.projectName = parts[1];
                    params.extensionType = parts[3];
                    params.extensionInstance = null; // Signal this is create mode
                } else {
                    view = 'extension-detail';
                    params.projectName = parts[1];
                    params.extensionType = parts[3];
                    params.extensionInstance = parts[4];
                }
            } else if (parts.length === 4 && parts[3]) {
                // Legacy fallback: project/{name}/extensions/{type-or-instance}
                // Try to determine if it's a type or instance
                view = 'extension-detail';
                params.projectName = parts[1];
                params.extensionType = parts[3];
                params.extensionInstance = null;
            }
        } else {
            view = 'project-detail';
            params.projectName = parts[1];
            params.tab = parts[2] || 'overview'; // Default to overview if no tab specified
        }
    } else if (hash.startsWith('team/')) {
        view = 'team-detail';
        params.teamName = hash.split('/')[1];
    } else if (hash.startsWith('deployment/')) {
        view = 'deployment-detail';
        const parts = hash.split('/');
        params.projectName = parts[1];
        params.deploymentId = parts[2];
    } else if (hash === 'teams') {
        view = 'teams';
    } else {
        view = 'projects';
    }

    return (
        <div className="min-h-screen flex flex-col">
            <Header
                user={user}
                onLogout={handleLogout}
                currentView={view}
                onShowGettingStarted={() => setShowGettingStarted(true)}
            />
            <main className="container mx-auto px-4 py-8 flex-1">
                {view === 'projects' && <ProjectsList />}
                {view === 'teams' && <TeamsList currentUser={user} />}
                {view === 'project-detail' && <ProjectDetail projectName={params.projectName} initialTab={params.tab} />}
                {view === 'team-detail' && <TeamDetail teamName={params.teamName} currentUser={user} />}
                {view === 'deployment-detail' && <DeploymentDetail projectName={params.projectName} deploymentId={params.deploymentId} />}
                {view === 'extension-detail' && <ExtensionDetailPage projectName={params.projectName} extensionType={params.extensionType} extensionInstance={params.extensionInstance} />}
            </main>
            <Footer version={version} />
            <GettingStartedModal
                isOpen={showGettingStarted}
                onClose={() => setShowGettingStarted(false)}
                publicUrl={CONFIG?.backendUrl}
                version={version?.version}
            />
        </div>
    );
}

// Initialize the React app
const root = ReactDOM.createRoot(document.getElementById('root'));
root.render(
    <ToastProvider>
        <App />
    </ToastProvider>
);
