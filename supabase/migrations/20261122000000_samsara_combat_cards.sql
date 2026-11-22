-- Execute uma vez no SQL Editor do Supabase para habilitar as fichas auxiliares.
create table if not exists public.samsara_combat_cards (
    id uuid primary key default gen_random_uuid(),
    name text not null,
    stats integer[] not null default array[0,0,0,0,0],
    pv integer not null default 10,
    pm integer not null default 5,
    created_at timestamptz not null default now(),
    constraint samsara_combat_cards_stats_length check (array_length(stats, 1) = 5),
    constraint samsara_combat_cards_pv_nonnegative check (pv >= 0),
    constraint samsara_combat_cards_pm_nonnegative check (pm >= 0)
);

alter table public.samsara_combat_cards enable row level security;

drop policy if exists "Master manages combat cards" on public.samsara_combat_cards;
create policy "Master manages combat cards"
on public.samsara_combat_cards
for all
to authenticated
using (auth.uid() = 'ac5dd4c6-6aa7-47a6-8192-703b44ef3961'::uuid)
with check (auth.uid() = 'ac5dd4c6-6aa7-47a6-8192-703b44ef3961'::uuid);

grant select, insert, update, delete on public.samsara_combat_cards to authenticated;
