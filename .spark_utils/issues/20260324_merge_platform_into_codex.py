"""One-off merge for standardizing voidm on a single shared SQLite database."""

from __future__ import annotations

import argparse
import shutil
import sqlite3
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


NODE_PROP_TABLES = (
    "graph_node_props_bool",
    "graph_node_props_int",
    "graph_node_props_json",
    "graph_node_props_real",
    "graph_node_props_text",
)

EDGE_PROP_TABLES = (
    "graph_edge_props_bool",
    "graph_edge_props_int",
    "graph_edge_props_json",
    "graph_edge_props_real",
    "graph_edge_props_text",
)


@dataclass
class MergeStats:
    memories: int = 0
    memory_scopes: int = 0
    memory_fts: int = 0
    memory_vectors: int = 0
    graph_nodes: int = 0
    graph_node_labels: int = 0
    graph_node_props: int = 0
    graph_edges: int = 0
    graph_edge_props: int = 0
    graph_property_keys: int = 0
    ontology_concepts: int = 0
    ontology_edges: int = 0
    ontology_ner_processed: int = 0
    ontology_merge_batch: int = 0
    ontology_merge_log: int = 0
    db_meta: int = 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--dest", required=True, type=Path)
    parser.add_argument("--backup-dir", required=True, type=Path)
    return parser.parse_args()


def connect(path: Path) -> sqlite3.Connection:
    conn = sqlite3.connect(path)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA foreign_keys = OFF")
    return conn


def ensure_exists(path: Path) -> None:
    if not path.exists():
        raise FileNotFoundError(f"Required database not found: {path}")


def backup_database(path: Path, backup_dir: Path, label: str) -> Path:
    backup_dir.mkdir(parents=True, exist_ok=True)
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    backup_path = backup_dir / f"{label}_{path.stem}_{timestamp}{path.suffix}"
    shutil.copy2(path, backup_path)
    return backup_path


def fetch_count(conn: sqlite3.Connection, table: str) -> int:
    return int(conn.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0])


def inserted(cursor: sqlite3.Cursor) -> int:
    return 1 if cursor.rowcount > 0 else 0


def load_key_map(conn: sqlite3.Connection) -> dict[str, int]:
    return {row["key"]: int(row["id"]) for row in conn.execute("SELECT id, key FROM graph_property_keys")}


def load_node_map(conn: sqlite3.Connection) -> dict[str, int]:
    return {
        row["memory_id"]: int(row["id"])
        for row in conn.execute("SELECT id, memory_id FROM graph_nodes")
    }


def assert_no_memory_conflicts(source: sqlite3.Connection, dest: sqlite3.Connection) -> None:
    source_rows = {
        row["id"]: row
        for row in source.execute(
            """
            SELECT id, type, content, importance, tags, metadata, quality_score, created_at, updated_at
            FROM memories
            """
        )
    }
    dest_rows = {
        row["id"]: row
        for row in dest.execute(
            """
            SELECT id, type, content, importance, tags, metadata, quality_score, created_at, updated_at
            FROM memories
            """
        )
    }

    conflicts: list[str] = []
    for memory_id, source_row in source_rows.items():
        dest_row = dest_rows.get(memory_id)
        if dest_row is None:
            continue
        source_tuple = tuple(source_row[key] for key in source_row.keys())
        dest_tuple = tuple(dest_row[key] for key in dest_row.keys())
        if source_tuple != dest_tuple:
            conflicts.append(memory_id)

    if conflicts:
        conflict_preview = ", ".join(conflicts[:5])
        raise RuntimeError(
            "Refusing to merge because memory IDs exist in both DBs with different content: "
            f"{conflict_preview}"
        )


def merge_memories(
    source: sqlite3.Connection,
    dest: sqlite3.Connection,
    stats: MergeStats,
) -> set[str]:
    dest_ids = {row["id"] for row in dest.execute("SELECT id FROM memories")}
    inserted_ids: set[str] = set()

    for row in source.execute(
        """
        SELECT id, type, content, importance, tags, metadata, quality_score, created_at, updated_at
        FROM memories
        ORDER BY created_at, id
        """
    ):
        if row["id"] in dest_ids:
            continue
        dest.execute(
            """
            INSERT INTO memories (
                id, type, content, importance, tags, metadata, quality_score, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            tuple(row[key] for key in row.keys()),
        )
        inserted_ids.add(str(row["id"]))
        stats.memories += 1

    return inserted_ids


def merge_memory_scopes(
    source: sqlite3.Connection,
    dest: sqlite3.Connection,
    stats: MergeStats,
) -> None:
    for row in source.execute("SELECT memory_id, scope FROM memory_scopes"):
        stats.memory_scopes += inserted(
            dest.execute(
                "INSERT OR IGNORE INTO memory_scopes (memory_id, scope) VALUES (?, ?)",
                (row["memory_id"], row["scope"]),
            )
        )


def merge_memory_fts(
    source: sqlite3.Connection,
    dest: sqlite3.Connection,
    inserted_ids: set[str],
    stats: MergeStats,
) -> None:
    for memory_id in sorted(inserted_ids):
        row = source.execute(
            "SELECT content FROM memories WHERE id = ?",
            (memory_id,),
        ).fetchone()
        if row is None:
            continue
        dest.execute(
            "INSERT INTO memories_fts (id, content) VALUES (?, ?)",
            (memory_id, row["content"]),
        )
        stats.memory_fts += 1


def merge_memory_vectors(
    source: sqlite3.Connection,
    dest: sqlite3.Connection,
    inserted_ids: set[str],
    stats: MergeStats,
) -> None:
    try:
        rows = source.execute("SELECT memory_id, embedding FROM vec_memories")
    except sqlite3.OperationalError as exc:
        message = str(exc)
        if "no such module: vec0" not in message and "no such table: vec_memories" not in message:
            raise
        print("Skipping direct vector copy; re-embed destination DB after merge.")
        return

    for row in rows:
        if row["memory_id"] not in inserted_ids:
            continue
        dest.execute(
            "INSERT OR IGNORE INTO vec_memories (memory_id, embedding) VALUES (?, ?)",
            (row["memory_id"], row["embedding"]),
        )
        stats.memory_vectors += 1


def merge_db_meta(
    source: sqlite3.Connection,
    dest: sqlite3.Connection,
    stats: MergeStats,
) -> None:
    for row in source.execute("SELECT key, value FROM db_meta"):
        stats.db_meta += inserted(
            dest.execute(
                "INSERT OR IGNORE INTO db_meta (key, value) VALUES (?, ?)",
                (row["key"], row["value"]),
            )
        )


def merge_graph_property_keys(
    source: sqlite3.Connection,
    dest: sqlite3.Connection,
    stats: MergeStats,
) -> None:
    for row in source.execute("SELECT key FROM graph_property_keys ORDER BY key"):
        stats.graph_property_keys += inserted(
            dest.execute(
                "INSERT OR IGNORE INTO graph_property_keys (key) VALUES (?)",
                (row["key"],),
            )
        )


def merge_graph_nodes(
    source: sqlite3.Connection,
    dest: sqlite3.Connection,
    stats: MergeStats,
) -> dict[str, int]:
    dest_node_map = load_node_map(dest)
    for row in source.execute("SELECT memory_id FROM graph_nodes ORDER BY memory_id"):
        if row["memory_id"] in dest_node_map:
            continue
        dest.execute(
            "INSERT OR IGNORE INTO graph_nodes (memory_id) VALUES (?)",
            (row["memory_id"],),
        )
        stats.graph_nodes += 1
    return load_node_map(dest)


def merge_graph_node_labels(
    source: sqlite3.Connection,
    dest: sqlite3.Connection,
    dest_node_map: dict[str, int],
    stats: MergeStats,
) -> None:
    for row in source.execute(
        """
        SELECT n.memory_id, l.label
        FROM graph_node_labels l
        JOIN graph_nodes n ON n.id = l.node_id
        """
    ):
        stats.graph_node_labels += inserted(
            dest.execute(
                "INSERT OR IGNORE INTO graph_node_labels (node_id, label) VALUES (?, ?)",
                (dest_node_map[row["memory_id"]], row["label"]),
            )
        )


def merge_graph_node_props(
    source: sqlite3.Connection,
    dest: sqlite3.Connection,
    dest_node_map: dict[str, int],
    dest_key_map: dict[str, int],
    stats: MergeStats,
) -> None:
    for table_name in NODE_PROP_TABLES:
        query = f"""
            SELECT n.memory_id, k.key, p.value
            FROM {table_name} p
            JOIN graph_nodes n ON n.id = p.node_id
            JOIN graph_property_keys k ON k.id = p.key_id
        """
        for row in source.execute(query):
            stats.graph_node_props += inserted(
                dest.execute(
                    f"INSERT OR IGNORE INTO {table_name} (node_id, key_id, value) VALUES (?, ?, ?)",
                    (
                        dest_node_map[row["memory_id"]],
                        dest_key_map[row["key"]],
                        row["value"],
                    ),
                )
            )


def merge_graph_edges(
    source: sqlite3.Connection,
    dest: sqlite3.Connection,
    dest_node_map: dict[str, int],
    stats: MergeStats,
) -> dict[int, int]:
    edge_map: dict[int, int] = {}
    query = """
        SELECT
            e.id AS edge_id,
            src.memory_id AS source_memory_id,
            dst.memory_id AS target_memory_id,
            e.rel_type,
            e.note,
            e.created_at
        FROM graph_edges e
        JOIN graph_nodes src ON src.id = e.source_id
        JOIN graph_nodes dst ON dst.id = e.target_id
    """
    for row in source.execute(query):
        source_node_id = dest_node_map[row["source_memory_id"]]
        target_node_id = dest_node_map[row["target_memory_id"]]
        stats.graph_edges += inserted(
            dest.execute(
                """
                INSERT OR IGNORE INTO graph_edges (source_id, target_id, rel_type, note, created_at)
                VALUES (?, ?, ?, ?, ?)
                """,
                (
                    source_node_id,
                    target_node_id,
                    row["rel_type"],
                    row["note"],
                    row["created_at"],
                ),
            )
        )
        dest_edge = dest.execute(
            """
            SELECT id
            FROM graph_edges
            WHERE source_id = ? AND target_id = ? AND rel_type = ?
            """,
            (source_node_id, target_node_id, row["rel_type"]),
        ).fetchone()
        edge_map[int(row["edge_id"])] = int(dest_edge["id"])
    return edge_map


def merge_graph_edge_props(
    source: sqlite3.Connection,
    dest: sqlite3.Connection,
    dest_key_map: dict[str, int],
    edge_map: dict[int, int],
    stats: MergeStats,
) -> None:
    for table_name in EDGE_PROP_TABLES:
        query = f"""
            SELECT p.edge_id, k.key, p.value
            FROM {table_name} p
            JOIN graph_property_keys k ON k.id = p.key_id
        """
        for row in source.execute(query):
            dest_edge_id = edge_map.get(int(row["edge_id"]))
            if dest_edge_id is None:
                continue
            stats.graph_edge_props += inserted(
                dest.execute(
                    f"INSERT OR IGNORE INTO {table_name} (edge_id, key_id, value) VALUES (?, ?, ?)",
                    (dest_edge_id, dest_key_map[row["key"]], row["value"]),
                )
            )


def merge_ontology(
    source: sqlite3.Connection,
    dest: sqlite3.Connection,
    stats: MergeStats,
) -> None:
    for row in source.execute(
        "SELECT id, name, description, scope, created_at FROM ontology_concepts ORDER BY id"
    ):
        stats.ontology_concepts += inserted(
            dest.execute(
                """
                INSERT OR IGNORE INTO ontology_concepts (id, name, description, scope, created_at)
                VALUES (?, ?, ?, ?, ?)
                """,
                tuple(row[key] for key in row.keys()),
            )
        )

    for row in source.execute(
        """
        SELECT from_id, from_type, rel_type, to_id, to_type, note, created_at
        FROM ontology_edges
        """
    ):
        stats.ontology_edges += inserted(
            dest.execute(
                """
                INSERT OR IGNORE INTO ontology_edges (
                    from_id, from_type, rel_type, to_id, to_type, note, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?)
                """,
                tuple(row[key] for key in row.keys()),
            )
        )

    for row in source.execute(
        "SELECT memory_id, processed_at, entity_count, link_count FROM ontology_ner_processed"
    ):
        stats.ontology_ner_processed += inserted(
            dest.execute(
                """
                INSERT OR IGNORE INTO ontology_ner_processed (
                    memory_id, processed_at, entity_count, link_count
                ) VALUES (?, ?, ?, ?)
                """,
                tuple(row[key] for key in row.keys()),
            )
        )

    for row in source.execute(
        """
        SELECT id, total_merges, failed_merges, conflicts, transaction_id, created_at, executed_at,
               rolled_back_at
        FROM ontology_merge_batch
        """
    ):
        stats.ontology_merge_batch += inserted(
            dest.execute(
                """
                INSERT OR IGNORE INTO ontology_merge_batch (
                    id, total_merges, failed_merges, conflicts, transaction_id, created_at,
                    executed_at, rolled_back_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                """,
                tuple(row[key] for key in row.keys()),
            )
        )

    for row in source.execute(
        """
        SELECT id, batch_id, source_id, target_id, edges_retargeted, conflicts_kept, status, reason,
               created_at, completed_at
        FROM ontology_merge_log
        """
    ):
        stats.ontology_merge_log += inserted(
            dest.execute(
                """
                INSERT OR IGNORE INTO ontology_merge_log (
                    id, batch_id, source_id, target_id, edges_retargeted, conflicts_kept, status,
                    reason, created_at, completed_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                tuple(row[key] for key in row.keys()),
            )
        )


def print_summary(
    source: sqlite3.Connection,
    dest: sqlite3.Connection,
    source_backup: Path,
    dest_backup: Path,
    stats: MergeStats,
) -> None:
    print("Merge complete")
    print(f"Source backup: {source_backup}")
    print(f"Destination backup: {dest_backup}")
    print(f"Source memory count: {fetch_count(source, 'memories')}")
    print(f"Destination memory count: {fetch_count(dest, 'memories')}")
    print("Inserted rows:")
    for name, value in stats.__dict__.items():
        print(f"  {name}: {value}")


def main() -> None:
    args = parse_args()
    ensure_exists(args.source)
    ensure_exists(args.dest)

    source_backup = backup_database(args.source, args.backup_dir, "platform")
    dest_backup = backup_database(args.dest, args.backup_dir, "codex")

    source = connect(args.source)
    dest = connect(args.dest)
    stats = MergeStats()

    try:
        assert_no_memory_conflicts(source, dest)
        dest.execute("BEGIN")
        merge_db_meta(source, dest, stats)
        inserted_ids = merge_memories(source, dest, stats)
        merge_memory_scopes(source, dest, stats)
        merge_memory_fts(source, dest, inserted_ids, stats)
        merge_memory_vectors(source, dest, inserted_ids, stats)
        merge_graph_property_keys(source, dest, stats)
        dest_key_map = load_key_map(dest)
        dest_node_map = merge_graph_nodes(source, dest, stats)
        merge_graph_node_labels(source, dest, dest_node_map, stats)
        merge_graph_node_props(source, dest, dest_node_map, dest_key_map, stats)
        edge_map = merge_graph_edges(source, dest, dest_node_map, stats)
        merge_graph_edge_props(source, dest, dest_key_map, edge_map, stats)
        merge_ontology(source, dest, stats)
        dest.commit()
        print_summary(source, dest, source_backup, dest_backup, stats)
    except Exception:
        dest.rollback()
        raise
    finally:
        source.close()
        dest.close()


if __name__ == "__main__":
    main()
