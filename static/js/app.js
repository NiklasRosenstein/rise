// React-based Rise Dashboard Application with Tailwind CSS
// Main application router with Header, LoginPage, and App components
const { useState, useEffect } = React;
// CONFIG is already defined in auth.js which loads before this script

// Header Component
function Header({ user, onLogout, currentView }) {
    const [isProfileOpen, setIsProfileOpen] = useState(false);
    const profileRef = React.useRef(null);
    const { showToast } = useToast();

    // Determine which section is active (projects or teams)
    const isProjectsActive = currentView === 'projects' || currentView === 'project-detail' || currentView === 'deployment-detail';
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
                    <a href="#projects" className="flex items-center gap-2 hover:opacity-80 transition-opacity">
                        <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                            <path d="M12 2L2 7L12 12L22 7L12 2Z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                            <path d="M2 17L12 22L22 17" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                            <path d="M2 12L12 17L22 12" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                        </svg>
                        <strong className="text-lg font-bold">Rise Dashboard</strong>
                    </a>
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
        view = 'project-detail';
        const parts = hash.split('/');
        params.projectName = parts[1];
        params.tab = parts[2] || 'overview'; // Default to overview if no tab specified
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
        <>
            <Header user={user} onLogout={handleLogout} currentView={view} />
            <main className="container mx-auto px-4 py-8">
                {view === 'projects' && <ProjectsList />}
                {view === 'teams' && <TeamsList currentUser={user} />}
                {view === 'project-detail' && <ProjectDetail projectName={params.projectName} initialTab={params.tab} />}
                {view === 'team-detail' && <TeamDetail teamName={params.teamName} currentUser={user} />}
                {view === 'deployment-detail' && <DeploymentDetail projectName={params.projectName} deploymentId={params.deploymentId} />}
            </main>
        </>
    );
}

// Initialize the React app
const root = ReactDOM.createRoot(document.getElementById('root'));
root.render(
    <ToastProvider>
        <App />
    </ToastProvider>
);
