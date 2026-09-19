import { invoke } from '@tauri-apps/api/core';
import {
  ChevronDown,
  ChevronRight,
  Database,
  MessageSquareText,
  RefreshCw,
  Search,
} from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import {
  ChatSessionDetail,
  ChatSessionSummary,
  ChatTurnDetail,
  DetailedTokenUsage,
} from '../types/usage';
import { formatNumber, formatUsd } from '../utils/cn';

function formatDate(timestamp: number | string | null): string {
  if (!timestamp) return 'Unknown date';
  const date = typeof timestamp === 'number'
    ? new Date(timestamp * 1000)
    : new Date(timestamp);
  if (Number.isNaN(date.getTime())) return 'Unknown date';
  return date.toLocaleString([], {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function UsageMetrics({ usage }: { usage: DetailedTokenUsage }) {
  const metrics = [
    ['Input', usage.inputTokens],
    ['Cached', usage.cachedInputTokens],
    ['Cache writes', usage.cacheWriteInputTokens],
    ['Uncached', usage.uncachedInputTokens],
    ['Output', usage.outputTokens],
    ['Reasoning', usage.reasoningTokens],
  ];

  return (
    <div className="grid grid-cols-2 md:grid-cols-3 xl:grid-cols-6 gap-2">
      {metrics.map(([label, value]) => (
        <div
          key={label}
          className="rounded-lg px-3 py-2"
          style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}
        >
          <span className="text-[10px] uppercase tracking-wider block mb-1" style={{ color: 'var(--text-muted)' }}>
            {label}
          </span>
          <span className="text-sm font-mono font-semibold tabular-nums" style={{ color: 'var(--text-primary)' }}>
            {formatNumber(value as number)}
          </span>
        </div>
      ))}
    </div>
  );
}

function TurnCard({
  turn,
  index,
  expanded,
  onToggle,
}: {
  turn: ChatTurnDetail;
  index: number;
  expanded: boolean;
  onToggle: () => void;
}) {
  const userMessage = turn.messages.find((message) => message.role === 'user');
  const title = userMessage?.text.replace(/\s+/g, ' ').trim() || `Turn ${index + 1}`;

  return (
    <article
      className="rounded-xl overflow-hidden"
      style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)' }}
    >
      <button
        type="button"
        onClick={onToggle}
        className="w-full p-4 text-left cursor-pointer"
        style={{ background: 'transparent' }}
      >
        <div className="flex items-start gap-3">
          <span className="mt-0.5" style={{ color: 'var(--text-muted)' }}>
            {expanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
          </span>
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2 mb-1">
              <span className="text-xs font-semibold" style={{ color: 'var(--purple-bright)' }}>
                Turn {index + 1}
              </span>
              <span className="text-[11px]" style={{ color: 'var(--text-muted)' }}>
                {formatDate(turn.startedAt)}
              </span>
              {(turn.model || turn.reasoningEffort) && (
                <span
                  className="text-[10px] px-2 py-0.5 rounded-full"
                  style={{ background: 'var(--purple-dim)', color: 'var(--purple-bright)' }}
                >
                  {turn.model ?? 'Unknown model'}
                  {turn.reasoningEffort ? ` · ${turn.reasoningEffort}` : ''}
                </span>
              )}
            </div>
            <p className="text-sm truncate" style={{ color: 'var(--text-primary)' }} title={title}>
              {title}
            </p>
          </div>
          <div className="text-right shrink-0">
            <span className="text-sm font-mono font-semibold block" style={{ color: 'var(--text-primary)' }}>
              {formatNumber(turn.usage.totalTokens)}
            </span>
            <span className="text-[10px]" style={{ color: 'var(--text-muted)' }}>
              {turn.cacheRate.toFixed(1)}% cache · {formatUsd(turn.estimatedCostUsd)}
            </span>
          </div>
        </div>
      </button>

      {expanded && (
        <div className="px-4 pb-4 space-y-4" style={{ borderTop: '1px solid var(--border-subtle)' }}>
          <div className="pt-4">
            <UsageMetrics usage={turn.usage} />
          </div>

          <section>
            <h4 className="text-[11px] uppercase tracking-wider font-semibold mb-2" style={{ color: 'var(--text-muted)' }}>
              Messages
            </h4>
            <div className="space-y-2">
              {turn.messages.map((message, messageIndex) => (
                <div
                  key={message.id ?? `${turn.id}-${messageIndex}`}
                  className="rounded-lg p-3"
                  style={{
                    background: message.role === 'user' ? 'var(--purple-dim)' : 'var(--bg-elevated)',
                    border: '1px solid var(--border-subtle)',
                  }}
                >
                  <div className="flex items-center justify-between gap-3 mb-2">
                    <span
                      className="text-[10px] uppercase tracking-wider font-bold"
                      style={{ color: message.role === 'user' ? 'var(--purple-bright)' : 'var(--text-muted)' }}
                    >
                      {message.role}
                    </span>
                    <span className="text-[10px]" style={{ color: 'var(--text-muted)' }}>
                      {formatDate(message.timestamp)}
                    </span>
                  </div>
                  <p
                    className="text-xs leading-relaxed whitespace-pre-wrap break-words max-h-64 overflow-y-auto"
                    style={{ color: 'var(--text-secondary)' }}
                  >
                    {message.text}
                  </p>
                </div>
              ))}
              {turn.messages.length === 0 && (
                <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
                  No user-visible message was stored for this turn.
                </p>
              )}
            </div>
          </section>

          <section>
            <div className="flex items-center justify-between gap-3 mb-2">
              <h4 className="text-[11px] uppercase tracking-wider font-semibold" style={{ color: 'var(--text-muted)' }}>
                Model requests
              </h4>
              <span className="text-[10px]" style={{ color: 'var(--text-muted)' }}>
                {turn.requests.length} request{turn.requests.length === 1 ? '' : 's'}
              </span>
            </div>
            <div className="overflow-x-auto rounded-lg" style={{ border: '1px solid var(--border-subtle)' }}>
              <table className="w-full text-xs min-w-[760px]">
                <thead style={{ background: 'var(--bg-elevated)' }}>
                  <tr style={{ color: 'var(--text-muted)' }}>
                    {['Time', 'Input', 'Cached', 'Write', 'Cache rate', 'Output', 'Reasoning', 'Total', 'API est.'].map((label) => (
                      <th key={label} className="px-3 py-2 text-right first:text-left font-medium">
                        {label}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {turn.requests.map((request, requestIndex) => (
                    <tr
                      key={`${turn.id}-request-${requestIndex}`}
                      style={{ borderTop: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}
                    >
                      <td className="px-3 py-2 whitespace-nowrap">
                        {request.timestamp
                          ? new Date(request.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })
                          : '—'}
                      </td>
                      <td className="px-3 py-2 text-right font-mono">{request.usage.inputTokens.toLocaleString()}</td>
                      <td className="px-3 py-2 text-right font-mono">{request.usage.cachedInputTokens.toLocaleString()}</td>
                      <td className="px-3 py-2 text-right font-mono">{request.usage.cacheWriteInputTokens.toLocaleString()}</td>
                      <td className="px-3 py-2 text-right font-mono" style={{ color: 'var(--green)' }}>
                        {request.cacheRate.toFixed(1)}%
                      </td>
                      <td className="px-3 py-2 text-right font-mono">{request.usage.outputTokens.toLocaleString()}</td>
                      <td className="px-3 py-2 text-right font-mono">{request.usage.reasoningTokens.toLocaleString()}</td>
                      <td className="px-3 py-2 text-right font-mono font-semibold">{request.usage.totalTokens.toLocaleString()}</td>
                      <td className="px-3 py-2 text-right font-mono" style={{ color: 'var(--purple-bright)' }}>
                        {formatUsd(request.estimatedCostUsd)}
                      </td>
                    </tr>
                  ))}
                  {turn.requests.length === 0 && (
                    <tr>
                      <td colSpan={9} className="px-3 py-5 text-center" style={{ color: 'var(--text-muted)' }}>
                        No token notification was stored for this turn.
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
          </section>
        </div>
      )}
    </article>
  );
}

export function ChatSessionHistory() {
  const [sessions, setSessions] = useState<ChatSessionSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<ChatSessionDetail | null>(null);
  const [query, setQuery] = useState('');
  const [loadingSessions, setLoadingSessions] = useState(true);
  const [loadingDetail, setLoadingDetail] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expandedTurns, setExpandedTurns] = useState<Set<string>>(new Set());

  const loadSessions = async () => {
    setLoadingSessions(true);
    setError(null);
    try {
      const rows = await invoke<ChatSessionSummary[]>('get_chat_sessions');
      setSessions(rows);
      setSelectedId((current) => current && rows.some((row) => row.id === current)
        ? current
        : rows[0]?.id ?? null);
    } catch (loadError) {
      setError(String(loadError));
    } finally {
      setLoadingSessions(false);
    }
  };

  useEffect(() => {
    void loadSessions();
  }, []);

  useEffect(() => {
    if (!selectedId) {
      setDetail(null);
      return;
    }
    let cancelled = false;
    setLoadingDetail(true);
    setError(null);
    invoke<ChatSessionDetail>('get_chat_session_detail', { sessionId: selectedId })
      .then((result) => {
        if (cancelled) return;
        setDetail(result);
        const latestTurn = result.turns[result.turns.length - 1];
        setExpandedTurns(latestTurn ? new Set([latestTurn.id]) : new Set());
      })
      .catch((loadError) => {
        if (!cancelled) setError(String(loadError));
      })
      .finally(() => {
        if (!cancelled) setLoadingDetail(false);
      });
    return () => {
      cancelled = true;
    };
  }, [selectedId]);

  const filteredSessions = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    if (!normalized) return sessions;
    return sessions.filter((session) =>
      [session.title, session.cwd, session.model, session.reasoningEffort]
        .filter(Boolean)
        .some((value) => value!.toLocaleLowerCase().includes(normalized))
    );
  }, [query, sessions]);

  return (
    <div className="grid grid-cols-1 lg:grid-cols-[360px_minmax(0,1fr)] gap-4 items-start">
      <aside
        className="rounded-xl overflow-hidden lg:sticky lg:top-4"
        style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)' }}
      >
        <div className="p-4" style={{ borderBottom: '1px solid var(--border-subtle)' }}>
          <div className="flex items-center justify-between gap-3 mb-3">
            <div>
              <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>
                Codex chats
              </h3>
              <span className="text-[11px]" style={{ color: 'var(--text-muted)' }}>
                {loadingSessions ? 'Loading local history…' : `${sessions.length} sessions`}
              </span>
            </div>
            <button
              type="button"
              onClick={() => void loadSessions()}
              disabled={loadingSessions}
              className="p-2 rounded-lg cursor-pointer disabled:opacity-50"
              style={{ background: 'var(--bg-elevated)', color: 'var(--text-muted)' }}
              title="Refresh chat history"
            >
              <RefreshCw size={14} className={loadingSessions ? 'animate-spin' : ''} />
            </button>
          </div>
          <label
            className="flex items-center gap-2 rounded-lg px-3 py-2"
            style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}
          >
            <Search size={14} style={{ color: 'var(--text-muted)' }} />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search chats, model, folder…"
              className="w-full bg-transparent outline-none text-xs"
              style={{ color: 'var(--text-primary)' }}
            />
          </label>
        </div>

        <div className="max-h-[720px] overflow-y-auto">
          {filteredSessions.map((session) => {
            const selected = session.id === selectedId;
            return (
              <button
                type="button"
                key={session.id}
                onClick={() => setSelectedId(session.id)}
                className="w-full p-4 text-left cursor-pointer"
                style={{
                  background: selected ? 'var(--purple-dim)' : 'transparent',
                  borderBottom: '1px solid var(--border-subtle)',
                }}
              >
                <div className="flex items-start gap-3">
                  <MessageSquareText
                    size={15}
                    className="mt-0.5 shrink-0"
                    style={{ color: selected ? 'var(--purple-bright)' : 'var(--text-muted)' }}
                  />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2 mb-1">
                      <span className="text-xs font-semibold truncate" style={{ color: 'var(--text-primary)' }}>
                        {session.title}
                      </span>
                      {session.isCurrent && (
                        <span
                          className="text-[9px] uppercase px-1.5 py-0.5 rounded-full shrink-0"
                          style={{ color: 'var(--green)', background: 'rgba(34, 197, 94, 0.12)' }}
                        >
                          Current
                        </span>
                      )}
                    </div>
                    <div className="text-[10px] mb-2 truncate" style={{ color: 'var(--text-muted)' }}>
                      {formatDate(session.updatedAt)} · {session.model ?? 'Unknown model'}
                      {session.reasoningEffort ? ` · ${session.reasoningEffort}` : ''}
                    </div>
                    <div className="flex items-center justify-between gap-2 text-[10px]">
                      <span className="font-mono" style={{ color: 'var(--text-secondary)' }}>
                        {formatNumber(session.usage.totalTokens)} tokens
                      </span>
                      <span className="font-mono" style={{ color: 'var(--green)' }}>
                        {session.cacheRate.toFixed(1)}% cache
                      </span>
                      <span className="font-mono" style={{ color: 'var(--purple-bright)' }}>
                        {formatUsd(session.estimatedCostUsd)}
                      </span>
                    </div>
                  </div>
                </div>
              </button>
            );
          })}
          {!loadingSessions && filteredSessions.length === 0 && (
            <div className="p-8 text-center text-xs" style={{ color: 'var(--text-muted)' }}>
              No matching Codex chats.
            </div>
          )}
        </div>
      </aside>

      <main className="min-w-0">
        {error && (
          <div
            className="rounded-xl p-4 mb-4 text-xs"
            style={{ color: 'var(--danger)', background: 'var(--danger-dim)', border: '1px solid rgba(255,77,94,.2)' }}
          >
            {error}
          </div>
        )}

        {loadingDetail && (
          <div
            className="rounded-xl p-12 text-center"
            style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)', color: 'var(--text-muted)' }}
          >
            <RefreshCw size={18} className="animate-spin mx-auto mb-3" />
            Reading messages and token requests…
          </div>
        )}

        {!loadingDetail && detail && (
          <div className="space-y-4">
            <section
              className="rounded-xl p-5"
              style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)' }}
            >
              <div className="flex flex-col xl:flex-row xl:items-start justify-between gap-4 mb-4">
                <div className="min-w-0">
                  <div className="flex items-center gap-2 mb-1">
                    <Database size={15} style={{ color: 'var(--purple)' }} />
                    <span className="text-[10px] uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>
                      Local Codex log
                    </span>
                  </div>
                  <h3 className="text-lg font-semibold break-words" style={{ color: 'var(--text-primary)' }}>
                    {detail.summary.title}
                  </h3>
                  <p className="text-[11px] mt-1 break-all" style={{ color: 'var(--text-muted)' }}>
                    {detail.summary.cwd ?? 'Unknown folder'} · {formatDate(detail.summary.createdAt)}
                  </p>
                </div>
                <div className="flex flex-wrap gap-2 shrink-0">
                  <span
                    className="text-xs px-2.5 py-1 rounded-full"
                    style={{ background: 'var(--purple-dim)', color: 'var(--purple-bright)' }}
                  >
                    {detail.summary.model ?? 'Unknown model'}
                    {detail.summary.reasoningEffort ? ` · ${detail.summary.reasoningEffort}` : ''}
                  </span>
                  <span
                    className="text-xs px-2.5 py-1 rounded-full"
                    style={{ background: 'var(--bg-elevated)', color: 'var(--text-secondary)' }}
                  >
                    {detail.summary.turnCount} turns · {detail.summary.requestCount} requests
                  </span>
                </div>
              </div>

              <div className="grid grid-cols-2 md:grid-cols-4 gap-2 mb-3">
                {[
                  ['Total tokens', formatNumber(detail.summary.usage.totalTokens), 'var(--text-primary)'],
                  ['Input cache rate', `${detail.summary.cacheRate.toFixed(1)}%`, 'var(--green)'],
                  ['Output tokens', formatNumber(detail.summary.usage.outputTokens), 'var(--text-primary)'],
                  ['API-equivalent est.', formatUsd(detail.summary.estimatedCostUsd), 'var(--purple-bright)'],
                ].map(([label, value, color]) => (
                  <div
                    key={label}
                    className="rounded-lg p-3"
                    style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}
                  >
                    <span className="text-[10px] uppercase tracking-wider block mb-1" style={{ color: 'var(--text-muted)' }}>
                      {label}
                    </span>
                    <span className="text-base font-mono font-semibold" style={{ color }}>
                      {value}
                    </span>
                  </div>
                ))}
              </div>
              <UsageMetrics usage={detail.summary.usage} />
              <p className="text-[10px] mt-3" style={{ color: 'var(--text-muted)' }}>
                Prices are API-equivalent estimates from token telemetry, not charges to your ChatGPT subscription.
                Reasoning tokens are included in output tokens and are shown separately for visibility.
              </p>
            </section>

            <div className="space-y-3">
              {detail.turns.map((turn, index) => (
                <TurnCard
                  key={turn.id}
                  turn={turn}
                  index={index}
                  expanded={expandedTurns.has(turn.id)}
                  onToggle={() => setExpandedTurns((current) => {
                    const next = new Set(current);
                    if (next.has(turn.id)) next.delete(turn.id);
                    else next.add(turn.id);
                    return next;
                  })}
                />
              ))}
              {detail.turns.length === 0 && (
                <div
                  className="rounded-xl p-8 text-center text-xs"
                  style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)', color: 'var(--text-muted)' }}
                >
                  This log has no stored user-visible turns.
                </div>
              )}
            </div>
          </div>
        )}

        {!loadingDetail && !detail && !error && (
          <div
            className="rounded-xl p-12 text-center text-xs"
            style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)', color: 'var(--text-muted)' }}
          >
            Select a Codex chat to inspect its messages and token requests.
          </div>
        )}
      </main>
    </div>
  );
}
