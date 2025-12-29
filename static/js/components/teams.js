// Team-related components for Rise Dashboard
// This file depends on React, utils.js, components/ui.js, and components/toast.js being loaded first

const { useState, useEffect, useCallback } = React;

// Teams List Component
function TeamsList({ currentUser }) {
    const [teams, setTeams] = useState([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);
    const [isModalOpen, setIsModalOpen] = useState(false);
    const [formData, setFormData] = useState({ name: '', members: '', owners: '' });
    const [saving, setSaving] = useState(false);
    const { showToast } = useToast();

    const loadTeams = useCallback(async () => {
        try {
            const data = await api.getTeams();
            setTeams(data);
        } catch (err) {
            setError(err.message);
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => {
        loadTeams();
    }, [loadTeams]);

    const handleCreateClick = () => {
        setFormData({ name: '', members: '', owners: currentUser?.email || '' });
        setIsModalOpen(true);
    };

    const handleCreate = async () => {
        if (!formData.name) {
            showToast('Team name is required', 'error');
            return;
        }

        // Parse comma-separated email lists
        const memberEmails = formData.members
            .split(',')
            .map(e => e.trim())
            .filter(e => e.length > 0);

        const ownerEmails = formData.owners
            .split(',')
            .map(e => e.trim())
            .filter(e => e.length > 0);

        if (ownerEmails.length === 0) {
            showToast('At least one owner is required', 'error');
            return;
        }

        setSaving(true);
        try {
            // Look up user IDs for owners and members
            const ownerLookup = await api.lookupUsers(ownerEmails);
            const memberLookup = memberEmails.length > 0 ? await api.lookupUsers(memberEmails) : { users: [] };

            if (!ownerLookup.users || ownerLookup.users.length !== ownerEmails.length) {
                showToast('One or more owner email addresses not found', 'error');
                setSaving(false);
                return;
            }

            if (memberEmails.length > 0 && (!memberLookup.users || memberLookup.users.length !== memberEmails.length)) {
                showToast('One or more member email addresses not found', 'error');
                setSaving(false);
                return;
            }

            const ownerIds = ownerLookup.users.map(u => u.id);
            const memberIds = memberLookup.users.map(u => u.id);

            await api.createTeam(formData.name, memberIds, ownerIds);
            showToast(`Team ${formData.name} created successfully`, 'success');
            setIsModalOpen(false);
            loadTeams();
        } catch (err) {
            showToast(`Failed to create team: ${err.message}`, 'error');
        } finally {
            setSaving(false);
        }
    };

    if (loading) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-600 dark:text-red-400">Error loading teams: {error}</p>;

    return (
        <section>
            <div className="flex justify-between items-center mb-6">
                <h2 className="text-2xl font-bold">Teams</h2>
                <Button variant="primary" size="sm" onClick={handleCreateClick}>
                    Create Team
                </Button>
            </div>
            <div className="bg-white dark:bg-gray-900 rounded-lg overflow-hidden border border-gray-200 dark:border-gray-800">
                <table className="w-full">
                    <thead className="bg-gray-100 dark:bg-gray-800">
                        <tr>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-700 dark:text-gray-300 uppercase tracking-wider">Name</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-700 dark:text-gray-300 uppercase tracking-wider">Members</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-700 dark:text-gray-300 uppercase tracking-wider">Owners</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-700 dark:text-gray-300 uppercase tracking-wider">Created</th>
                        </tr>
                    </thead>
                    <tbody className="divide-y divide-gray-200 dark:divide-gray-800">
                        {teams.length === 0 ? (
                            <tr>
                                <td colSpan="4" className="px-6 py-8 text-center text-gray-600 dark:text-gray-400">
                                    No teams found.
                                </td>
                            </tr>
                        ) : (
                            teams.map(t => (
                                <tr
                                    key={t.id}
                                    onClick={() => window.location.hash = `team/${t.name}`}
                                    className="hover:bg-gray-100 dark:bg-gray-800/50 transition-colors cursor-pointer"
                                >
                                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-100">
                                        <div className="flex items-center gap-2">
                                            {t.name}
                                            {t.idp_managed && (
                                                <span className="text-xs bg-purple-600 text-white px-2 py-0.5 rounded">IDP</span>
                                            )}
                                        </div>
                                    </td>
                                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-700 dark:text-gray-300">{t.members.length}</td>
                                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-700 dark:text-gray-300">{t.owners.length}</td>
                                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-700 dark:text-gray-300">{formatDate(t.created)}</td>
                                </tr>
                            ))
                        )}
                    </tbody>
                </table>
            </div>

            <Modal
                isOpen={isModalOpen}
                onClose={() => setIsModalOpen(false)}
                title="Create Team"
            >
                <div className="space-y-4">
                    <FormField
                        label="Team Name"
                        id="team-name"
                        value={formData.name}
                        onChange={(e) => setFormData({ ...formData, name: e.target.value })}
                        placeholder="engineering"
                        required
                    />

                    <FormField
                        label="Owners (emails, comma-separated)"
                        id="team-owners"
                        type="textarea"
                        value={formData.owners}
                        onChange={(e) => setFormData({ ...formData, owners: e.target.value })}
                        placeholder="alice@example.com, bob@example.com"
                        required
                        rows={3}
                    />
                    <p className="text-sm text-gray-600 dark:text-gray-500 -mt-2">
                        Owners can manage the team. At least one owner is required.
                    </p>

                    <FormField
                        label="Members (emails, comma-separated)"
                        id="team-members"
                        type="textarea"
                        value={formData.members}
                        onChange={(e) => setFormData({ ...formData, members: e.target.value })}
                        placeholder="charlie@example.com, dana@example.com"
                        rows={3}
                    />
                    <p className="text-sm text-gray-600 dark:text-gray-500 -mt-2">
                        Members can use the team for project ownership.
                    </p>

                    <div className="flex justify-end gap-3 pt-4">
                        <Button
                            variant="secondary"
                            onClick={() => setIsModalOpen(false)}
                            disabled={saving}
                        >
                            Cancel
                        </Button>
                        <Button
                            variant="primary"
                            onClick={handleCreate}
                            loading={saving}
                        >
                            Create
                        </Button>
                    </div>
                </div>
            </Modal>
        </section>
    );
}

// Team Detail Component
function TeamDetail({ teamName, currentUser }) {
    const [team, setTeam] = useState(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);
    const [newOwnerEmail, setNewOwnerEmail] = useState('');
    const [newMemberEmail, setNewMemberEmail] = useState('');
    const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
    const [deleting, setDeleting] = useState(false);
    const { showToast } = useToast();

    const loadTeam = useCallback(async () => {
        try {
            const data = await api.getTeam(teamName, { expand: 'members,owners' });
            setTeam(data);
        } catch (err) {
            setError(err.message);
        } finally {
            setLoading(false);
        }
    }, [teamName]);

    useEffect(() => {
        loadTeam();
    }, [loadTeam]);

    // Check if user can manage this team
    const canManage = currentUser && team && (
        currentUser.is_admin ||
        (team.owners && team.owners.some(o => o.email === currentUser.email))
    );

    // IDP-managed teams can only be managed by admins
    const canEdit = canManage && (!team?.idp_managed || currentUser?.is_admin);

    const handleAddOwner = async () => {
        if (!newOwnerEmail.trim()) {
            showToast('Please enter an email address', 'error');
            return;
        }

        try {
            // Look up user ID by email
            const lookupResult = await api.lookupUsers([newOwnerEmail.trim()]);
            if (!lookupResult.users || lookupResult.users.length === 0) {
                showToast(`User with email ${newOwnerEmail} not found`, 'error');
                return;
            }

            const currentOwnerIds = team.owners?.map(o => o.id) || [];
            const newOwnerId = lookupResult.users[0].id;

            await api.updateTeam(team.id, {
                owners: [...currentOwnerIds, newOwnerId]
            });
            showToast(`Added ${newOwnerEmail} as owner`, 'success');
            setNewOwnerEmail('');
            await loadTeam();
        } catch (err) {
            showToast(`Failed to add owner: ${err.message}`, 'error');
        }
    };

    const handleRemoveOwner = async (ownerId, email) => {
        try {
            const currentOwnerIds = team.owners?.map(o => o.id) || [];
            const updatedOwnerIds = currentOwnerIds.filter(id => id !== ownerId);

            if (updatedOwnerIds.length === 0) {
                showToast('Cannot remove last owner', 'error');
                return;
            }

            await api.updateTeam(team.id, { owners: updatedOwnerIds });
            showToast(`Removed ${email} from owners`, 'success');
            await loadTeam();
        } catch (err) {
            showToast(`Failed to remove owner: ${err.message}`, 'error');
        }
    };

    const handleAddMember = async () => {
        if (!newMemberEmail.trim()) {
            showToast('Please enter an email address', 'error');
            return;
        }

        try {
            // Look up user ID by email
            const lookupResult = await api.lookupUsers([newMemberEmail.trim()]);
            if (!lookupResult.users || lookupResult.users.length === 0) {
                showToast(`User with email ${newMemberEmail} not found`, 'error');
                return;
            }

            const currentMemberIds = team.members?.map(m => m.id) || [];
            const newMemberId = lookupResult.users[0].id;

            await api.updateTeam(team.id, {
                members: [...currentMemberIds, newMemberId]
            });
            showToast(`Added ${newMemberEmail} as member`, 'success');
            setNewMemberEmail('');
            await loadTeam();
        } catch (err) {
            showToast(`Failed to add member: ${err.message}`, 'error');
        }
    };

    const handleRemoveMember = async (memberId, email) => {
        try {
            const currentMemberIds = team.members?.map(m => m.id) || [];
            const updatedMemberIds = currentMemberIds.filter(id => id !== memberId);
            await api.updateTeam(team.id, { members: updatedMemberIds });
            showToast(`Removed ${email} from members`, 'success');
            await loadTeam();
        } catch (err) {
            showToast(`Failed to remove member: ${err.message}`, 'error');
        }
    };

    const handleDeleteTeam = async () => {
        setDeleting(true);
        try {
            await api.deleteTeam(team.id);
            showToast(`Team ${team.name} deleted successfully`, 'success');
            window.location.hash = '#teams';
        } catch (err) {
            showToast(`Failed to delete team: ${err.message}`, 'error');
            setDeleting(false);
        }
    };

    if (loading) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-600 dark:text-red-400">Error loading team: {error}</p>;
    if (!team) return <p className="text-gray-600 dark:text-gray-400">Team not found.</p>;

    return (
        <section>
            <a href="#teams" className="inline-flex items-center gap-2 text-indigo-600 dark:text-indigo-400 hover:text-indigo-700 dark:hover:text-indigo-300 mb-6 transition-colors">
                ← Back to Teams
            </a>

            <div className="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-lg p-6 mb-6">
                <div className="flex justify-between items-start mb-4">
                    <div>
                        <div className="flex items-center gap-2">
                            <h3 className="text-2xl font-bold">Team {team.name}</h3>
                            {team.idp_managed && (
                                <span className="text-xs bg-purple-600 text-white px-2 py-1 rounded">IDP</span>
                            )}
                        </div>
                    </div>
                    {canEdit && (
                        <Button
                            variant="danger"
                            size="sm"
                            onClick={() => setDeleteDialogOpen(true)}
                        >
                            Delete Team
                        </Button>
                    )}
                </div>
                <dl className="grid grid-cols-2 gap-4 text-sm">
                    <div>
                        <dt className="text-gray-600 dark:text-gray-400">Created</dt>
                        <dd className="mt-1 text-gray-900 dark:text-gray-200">{formatDate(team.created)}</dd>
                    </div>
                    <div>
                        <dt className="text-gray-600 dark:text-gray-400">Updated</dt>
                        <dd className="mt-1 text-gray-900 dark:text-gray-200">{formatDate(team.updated)}</dd>
                    </div>
                </dl>
                {team.idp_managed && !currentUser?.is_admin && (
                    <div className="mt-4 p-3 bg-purple-900/20 border border-purple-700 rounded text-sm text-purple-300">
                        This team is managed by your identity provider and can only be modified by administrators.
                    </div>
                )}
            </div>

            <div className="mb-6">
                <div className="flex justify-between items-center mb-4">
                    <h4 className="text-lg font-bold">Owners</h4>
                </div>
                {team.owners && team.owners.length > 0 ? (
                    <div className="bg-white dark:bg-gray-900 rounded-lg overflow-hidden border border-gray-200 dark:border-gray-800 mb-4">
                        <table className="w-full">
                            <thead className="bg-gray-100 dark:bg-gray-800">
                                <tr>
                                    <th className="px-6 py-3 text-left text-xs font-medium text-gray-700 dark:text-gray-300 uppercase tracking-wider">Email</th>
                                    {canEdit && <th className="px-6 py-3 text-right text-xs font-medium text-gray-700 dark:text-gray-300 uppercase tracking-wider">Actions</th>}
                                </tr>
                            </thead>
                            <tbody className="divide-y divide-gray-200 dark:divide-gray-800">
                                {team.owners.map(owner => (
                                    <tr key={owner.id} className="hover:bg-gray-100 dark:bg-gray-800/50 transition-colors">
                                        <td className="px-6 py-4 text-sm text-gray-900 dark:text-gray-200">{owner.email}</td>
                                        {canEdit && (
                                            <td className="px-6 py-4">
                                                <div className="flex justify-end">
                                                    <Button
                                                        variant="danger"
                                                        size="sm"
                                                        onClick={() => handleRemoveOwner(owner.id, owner.email)}
                                                    >
                                                        Remove
                                                    </Button>
                                                </div>
                                            </td>
                                        )}
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    </div>
                ) : (
                    <p className="text-gray-600 dark:text-gray-400 mb-4">No owners</p>
                )}
                {canEdit && (
                    <div className="flex justify-end">
                        <div className="flex gap-2">
                            <input
                                type="email"
                                placeholder="owner@example.com"
                                value={newOwnerEmail}
                                onChange={(e) => setNewOwnerEmail(e.target.value)}
                                onKeyPress={(e) => e.key === 'Enter' && handleAddOwner()}
                                className="w-64 bg-gray-100 dark:bg-gray-800 border border-gray-300 dark:border-gray-700 rounded px-3 py-2 text-sm text-gray-900 dark:text-gray-200 placeholder-gray-500 focus:outline-none focus:border-indigo-500"
                            />
                            <Button variant="primary" size="sm" onClick={handleAddOwner}>
                                Add
                            </Button>
                        </div>
                    </div>
                )}
            </div>

            <div>
                <div className="flex justify-between items-center mb-4">
                    <h4 className="text-lg font-bold">Members</h4>
                </div>
                {team.members && team.members.length > 0 ? (
                    <div className="bg-white dark:bg-gray-900 rounded-lg overflow-hidden border border-gray-200 dark:border-gray-800 mb-4">
                        <table className="w-full">
                            <thead className="bg-gray-100 dark:bg-gray-800">
                                <tr>
                                    <th className="px-6 py-3 text-left text-xs font-medium text-gray-700 dark:text-gray-300 uppercase tracking-wider">Email</th>
                                    {canEdit && <th className="px-6 py-3 text-right text-xs font-medium text-gray-700 dark:text-gray-300 uppercase tracking-wider">Actions</th>}
                                </tr>
                            </thead>
                            <tbody className="divide-y divide-gray-200 dark:divide-gray-800">
                                {team.members.map(member => (
                                    <tr key={member.id} className="hover:bg-gray-100 dark:bg-gray-800/50 transition-colors">
                                        <td className="px-6 py-4 text-sm text-gray-900 dark:text-gray-200">{member.email}</td>
                                        {canEdit && (
                                            <td className="px-6 py-4">
                                                <div className="flex justify-end">
                                                    <Button
                                                        variant="danger"
                                                        size="sm"
                                                        onClick={() => handleRemoveMember(member.id, member.email)}
                                                    >
                                                        Remove
                                                    </Button>
                                                </div>
                                            </td>
                                        )}
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    </div>
                ) : (
                    <p className="text-gray-600 dark:text-gray-400 mb-4">No members</p>
                )}
                {canEdit && (
                    <div className="flex justify-end">
                        <div className="flex gap-2">
                            <input
                                type="email"
                                placeholder="member@example.com"
                                value={newMemberEmail}
                                onChange={(e) => setNewMemberEmail(e.target.value)}
                                onKeyPress={(e) => e.key === 'Enter' && handleAddMember()}
                                className="w-64 bg-gray-100 dark:bg-gray-800 border border-gray-300 dark:border-gray-700 rounded px-3 py-2 text-sm text-gray-900 dark:text-gray-200 placeholder-gray-500 focus:outline-none focus:border-indigo-500"
                            />
                            <Button variant="primary" size="sm" onClick={handleAddMember}>
                                Add
                            </Button>
                        </div>
                    </div>
                )}
            </div>

            {deleteDialogOpen && (
                <Modal
                    isOpen={deleteDialogOpen}
                    onClose={() => setDeleteDialogOpen(false)}
                    title="Delete Team"
                >
                    <div className="mb-6">
                        <p className="text-gray-700 dark:text-gray-300 mb-4">
                            Are you sure you want to delete team <strong className="text-white">{team.name}</strong>?
                        </p>
                        <p className="text-sm text-gray-600 dark:text-gray-400">
                            This action cannot be undone.
                        </p>
                    </div>
                    <div className="flex justify-end gap-3">
                        <Button
                            variant="secondary"
                            onClick={() => setDeleteDialogOpen(false)}
                            disabled={deleting}
                        >
                            Cancel
                        </Button>
                        <Button
                            variant="danger"
                            onClick={handleDeleteTeam}
                            disabled={deleting}
                        >
                            {deleting ? 'Deleting...' : 'Delete Team'}
                        </Button>
                    </div>
                </Modal>
            )}
        </section>
    );
}
