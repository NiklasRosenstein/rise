-- Allow users to have multiple roles on the same team
-- Changes primary key from (team_id, user_id) to (team_id, user_id, role)
-- This allows a user to be both an owner and a member of the same team

ALTER TABLE team_members DROP CONSTRAINT team_members_pkey;
ALTER TABLE team_members ADD PRIMARY KEY (team_id, user_id, role);
