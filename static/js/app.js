// React-based Rise Dashboard Application with Tailwind CSS
// Main application router with Header, LoginPage, and App components
const { useState, useEffect } = React;
// CONFIG is already defined in auth.js which loads before this script

// Header Component
function Header({ user, onLogout, currentView, onShowGettingStarted }) {
    const [isProfileOpen, setIsProfileOpen] = useState(false);
    const [theme, setTheme] = useState('system');
    const profileRef = React.useRef(null);
    const { showToast } = useToast();

    // Determine which section is active (projects or teams)
    const isProjectsActive = currentView === 'projects' || currentView === 'project-detail' || currentView === 'deployment-detail' || currentView === 'extension-detail';
    const isTeamsActive = currentView === 'teams' || currentView === 'team-detail';

    // Initialize theme from localStorage or default to system
    useEffect(() => {
        const savedTheme = localStorage.getItem('rise_theme') || 'system';
        setTheme(savedTheme);
        applyTheme(savedTheme);
    }, []);

    // Apply theme to document
    const applyTheme = (selectedTheme) => {
        const root = document.documentElement;

        if (selectedTheme === 'system') {
            // Use system preference
            const systemPrefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
            root.classList.toggle('dark', systemPrefersDark);
        } else if (selectedTheme === 'dark') {
            root.classList.add('dark');
        } else {
            root.classList.remove('dark');
        }
    };

    // Handle theme change
    const handleThemeChange = (newTheme) => {
        setTheme(newTheme);
        localStorage.setItem('rise_theme', newTheme);
        applyTheme(newTheme);
        setIsProfileOpen(false);
    };

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

    return (
        <header className="bg-white dark:bg-gray-900 border-b border-gray-200 dark:border-gray-800">
            <nav className="container mx-auto px-4 py-4">
                <div className="flex items-center justify-between">
                    <div className="flex items-center gap-4">
                        <a href="#projects" className="flex items-center gap-2 hover:opacity-80 transition-opacity">
                            <div className="w-5 h-5 svg-mask" style={{
                                maskImage: 'url(/assets/logo.svg)',
                                WebkitMaskImage: 'url(/assets/logo.svg)'
                            }}></div>
                            <strong className="text-lg font-bold">Rise Dashboard</strong>
                        </a>
                        <button
                            onClick={onShowGettingStarted}
                            className="flex items-center gap-2 px-3 py-1.5 text-sm font-medium text-white bg-indigo-600 hover:bg-indigo-700 rounded-md transition-colors"
                        >
                            <div className="w-4 h-4 svg-mask" style={{
                                maskImage: 'url(/assets/lightning.svg)',
                                WebkitMaskImage: 'url(/assets/lightning.svg)'
                            }}></div>
                            Getting Started
                        </button>
                    </div>
                    <div className="flex items-center gap-6">
                        <a
                            href="#projects"
                            className={`transition-colors ${isProjectsActive ? 'text-indigo-600 dark:text-indigo-400 font-semibold' : 'text-gray-700 dark:text-gray-300 hover:text-gray-900 dark:hover:text-white'}`}
                        >
                            Projects
                        </a>
                        <a
                            href="#teams"
                            className={`transition-colors ${isTeamsActive ? 'text-indigo-600 dark:text-indigo-400 font-semibold' : 'text-gray-700 dark:text-gray-300 hover:text-gray-900 dark:hover:text-white'}`}
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
                                    <div className="w-5 h-5 svg-mask text-white" style={{
                                        maskImage: 'url(/assets/user.svg)',
                                        WebkitMaskImage: 'url(/assets/user.svg)'
                                    }}></div>
                                </div>
                            </button>

                            {isProfileOpen && (
                                <div className="absolute right-0 mt-2 w-64 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-xl z-50">
                                    <div className="p-4 border-b border-gray-200 dark:border-gray-700">
                                        <p className="text-sm text-gray-600 dark:text-gray-400 mb-1">Signed in as</p>
                                        <p className="text-gray-900 dark:text-white font-medium break-all">{user?.email}</p>
                                    </div>
                                    <div className="p-2 border-t border-gray-200 dark:border-gray-700">
                                        <div className="px-3 py-2">
                                            <p className="text-xs text-gray-600 dark:text-gray-400 mb-2">Theme</p>
                                            <div className="space-y-1">
                                                <button
                                                    onClick={() => handleThemeChange('system')}
                                                    className={`w-full flex items-center gap-2 px-2 py-1.5 text-left text-sm rounded transition-colors ${
                                                        theme === 'system' ? 'bg-indigo-600 text-white' : 'text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700'
                                                    }`}
                                                >
                                                    <div className="w-4 h-4 svg-mask" style={{
                                                        maskImage: 'url(/assets/theme-system.svg)',
                                                        WebkitMaskImage: 'url(/assets/theme-system.svg)'
                                                    }}></div>
                                                    System
                                                </button>
                                                <button
                                                    onClick={() => handleThemeChange('light')}
                                                    className={`w-full flex items-center gap-2 px-2 py-1.5 text-left text-sm rounded transition-colors ${
                                                        theme === 'light' ? 'bg-indigo-600 text-white' : 'text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700'
                                                    }`}
                                                >
                                                    <div className="w-4 h-4 svg-mask" style={{
                                                        maskImage: 'url(/assets/theme-light.svg)',
                                                        WebkitMaskImage: 'url(/assets/theme-light.svg)'
                                                    }}></div>
                                                    Light
                                                </button>
                                                <button
                                                    onClick={() => handleThemeChange('dark')}
                                                    className={`w-full flex items-center gap-2 px-2 py-1.5 text-left text-sm rounded transition-colors ${
                                                        theme === 'dark' ? 'bg-indigo-600 text-white' : 'text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700'
                                                    }`}
                                                >
                                                    <div className="w-4 h-4 svg-mask" style={{
                                                        maskImage: 'url(/assets/theme-dark.svg)',
                                                        WebkitMaskImage: 'url(/assets/theme-dark.svg)'
                                                    }}></div>
                                                    Dark
                                                </button>
                                            </div>
                                        </div>
                                    </div>
                                    <div className="p-2 border-t border-gray-200 dark:border-gray-700">
                                        <button
                                            onClick={() => { setIsProfileOpen(false); onLogout(); }}
                                            className="w-full flex items-center gap-2 px-3 py-2 text-left text-red-600 dark:text-red-400 hover:bg-gray-100 dark:hover:bg-gray-700 rounded transition-colors"
                                        >
                                            <div className="w-4 h-4 svg-mask" style={{
                                                maskImage: 'url(/assets/logout.svg)',
                                                WebkitMaskImage: 'url(/assets/logout.svg)'
                                            }}></div>
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
            <div className="space-y-4 text-gray-700 dark:text-gray-300">
                <p>This is how you get started with your first Rise project:</p>

                <div className="bg-gray-100 dark:bg-gray-800 rounded-lg p-4 space-y-4">
                    <div>
                        <h4 className="text-sm font-semibold text-gray-600 dark:text-gray-400 mb-2"># Install the Rise CLI and log-in</h4>
                        <pre className="text-sm text-indigo-300 overflow-x-auto whitespace-pre-wrap">$ {installCommand}{'\n'}$ rise login --url {publicUrl || window.location.origin}</pre>
                    </div>

                    <div>
                        <h4 className="text-sm font-semibold text-gray-600 dark:text-gray-400 mb-2"># Deploy a sample project</h4>
                        <pre className="text-sm text-indigo-300 overflow-x-auto whitespace-pre-wrap">$ git clone https://github.com/GoogleCloudPlatform/buildpack-samples{'\n'}$ rise project create my-project # Pick a unique project name{'\n'}$ rise deployment create my-project buildpack-samples/sample-python/</pre>
                    </div>
                </div>

                <p className="text-sm text-gray-600 dark:text-gray-400 mt-4">
                    For more information, visit the{' '}
                    <a
                        href="https://github.com/NiklasRosenstein/rise"
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-indigo-600 dark:text-indigo-400 hover:text-indigo-700 dark:hover:text-indigo-300 underline"
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

    const handleLogin = async () => {
        setStatus('Redirecting to login...');
        setLoading(true);
        try {
            await login();
        } catch (error) {
            setStatus(`Error: ${error.message}`);
            setLoading(false);
        }
    };

    return (
        <div className="flex items-center justify-center min-h-screen bg-gradient-to-br from-gray-50 via-gray-100 to-gray-200 dark:from-gray-900 dark:via-gray-950 dark:to-black">
            <div className="w-full max-w-md p-8 bg-white dark:bg-gray-900 rounded-lg border border-gray-200 dark:border-gray-800 shadow-2xl">
                <div className="text-center mb-8">
                    <div className="flex justify-center mb-4">
                        <div className="w-16 h-16 svg-mask text-indigo-500" style={{
                            maskImage: 'url(/assets/logo.svg)',
                            WebkitMaskImage: 'url(/assets/logo.svg)'
                        }}></div>
                    </div>
                    <h1 className="text-3xl font-bold text-gray-900 dark:text-white mb-2">Rise</h1>
                    <p className="text-gray-600 dark:text-gray-400">Container Deployment Platform</p>
                </div>

                {loading ? (
                    <div className="flex flex-col items-center gap-4 py-8">
                        <div className="w-12 h-12 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div>
                        <p className="text-gray-700 dark:text-gray-300">{status}</p>
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
                            <p className="text-center text-sm text-red-600 dark:text-red-400">{status}</p>
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
    const { showToast } = useToast();

    useEffect(() => {
        // Check if we're handling extension OAuth callback (tokens in hash fragment)
        if (window.location.hash && (window.location.hash.includes('access_token=') || window.location.hash.includes('error='))) {
            const fragment = window.location.hash.substring(1);
            const params = new URLSearchParams(fragment);

            // Restore the original page location from sessionStorage
            const returnPath = sessionStorage.getItem('oauth_return_path');

            const error = params.get('error');
            const errorDescription = params.get('error_description');
            const accessToken = params.get('access_token');

            if (error) {
                // OAuth flow failed
                const message = errorDescription || `OAuth flow failed: ${error}`;
                showToast(message, 'error');
            } else if (accessToken) {
                // OAuth flow succeeded
                const expiresIn = params.get('expires_in');
                const expiresAt = params.get('expires_at');

                // Calculate expiration time
                let expiresAtDate;
                if (expiresAt) {
                    expiresAtDate = new Date(expiresAt);
                } else if (expiresIn) {
                    expiresAtDate = new Date(Date.now() + parseInt(expiresIn) * 1000);
                }

                // Show success toast
                const message = `OAuth flow successful! Token expires ${expiresAtDate ? expiresAtDate.toLocaleString() : 'soon'}`;
                showToast(message, 'success');
            }

            // Clean up sessionStorage
            sessionStorage.removeItem('oauth_return_path');

            // Navigate back to the extension page
            if (returnPath) {
                window.location.hash = returnPath;
            } else {
                // Fallback to home if no return path
                window.location.hash = '';
            }

            // Continue with normal auth check - don't skip it!
            // This ensures the user stays logged in after OAuth extension callback
        }

        async function loadUser() {
            try {
                const userData = await api.getMe();
                setUser(userData);
            } catch (err) {
                // If getMe fails with 401, user is not authenticated
                // For other errors, also show login as we can't proceed without user data
                console.error('Failed to load user:', err);
                setUser(null);
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

    if (!user) {
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
        <div className="min-h-screen flex flex-col bg-gray-50 dark:bg-gray-950">
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
