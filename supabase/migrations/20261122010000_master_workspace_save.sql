-- Execute no SQL Editor do Supabase para habilitar o salvamento mestre.
alter table public.samsara_combat_cards
    add column if not exists pv_max integer not null default 10,
    add column if not exists pm_max integer not null default 5;

update public.samsara_combat_cards set pv_max = pv where pv_max = 10 and pv <> 10;
update public.samsara_combat_cards set pm_max = pm where pm_max = 5 and pm <> 5;

create table if not exists public.samsara_master_settings (
    id integer primary key check (id = 1),
    notes text not null default ''
);

alter table public.samsara_master_settings enable row level security;
drop policy if exists "Master manages master settings" on public.samsara_master_settings;
create policy "Master manages master settings"
on public.samsara_master_settings for all to authenticated
using (auth.uid() = 'ac5dd4c6-6aa7-47a6-8192-703b44ef3961'::uuid)
with check (auth.uid() = 'ac5dd4c6-6aa7-47a6-8192-703b44ef3961'::uuid);

grant select, insert, update on public.samsara_master_settings to authenticated;
insert into public.samsara_master_settings (id, notes)
values (1, '')
on conflict (id) do nothing;
