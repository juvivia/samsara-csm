-- SAMSARA: estado compartilhado de combate e ordem de turnos
-- Execute uma única vez no SQL Editor do Supabase.

create table if not exists public.samsara_combat_state (
    id integer primary key default 1 check (id = 1),
    is_active boolean not null default false,
    participant_user_ids uuid[] not null default '{}',
    turn_order jsonb not null default '[]'::jsonb,
    current_index integer not null default 0,
    round integer not null default 1,
    updated_at timestamptz not null default now()
);

alter table public.samsara_combat_state enable row level security;

grant select, insert, update on public.samsara_combat_state to authenticated;

drop policy if exists "samsara_combat_master_select" on public.samsara_combat_state;
create policy "samsara_combat_master_select"
on public.samsara_combat_state for select to authenticated
using (auth.uid() = 'ac5dd4c6-6aa7-47a6-8192-703b44ef3961'::uuid);

drop policy if exists "samsara_combat_players_select" on public.samsara_combat_state;
create policy "samsara_combat_players_select"
on public.samsara_combat_state for select to authenticated
using (auth.uid() = any(participant_user_ids));

drop policy if exists "samsara_combat_master_insert" on public.samsara_combat_state;
create policy "samsara_combat_master_insert"
on public.samsara_combat_state for insert to authenticated
with check (auth.uid() = 'ac5dd4c6-6aa7-47a6-8192-703b44ef3961'::uuid);

drop policy if exists "samsara_combat_master_update" on public.samsara_combat_state;
create policy "samsara_combat_master_update"
on public.samsara_combat_state for update to authenticated
using (auth.uid() = 'ac5dd4c6-6aa7-47a6-8192-703b44ef3961'::uuid)
with check (auth.uid() = 'ac5dd4c6-6aa7-47a6-8192-703b44ef3961'::uuid);
