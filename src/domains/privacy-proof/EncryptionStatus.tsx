import { useEffect, useState } from "react";
import { getEncryptionStatement, type EncryptionStatement } from "@/shared/ipc/tauri";
import "./EncryptionStatus.css";

export function EncryptionStatus() {
    const [statement, setStatement] = useState<EncryptionStatement | null>(null);
    const [failed, setFailed] = useState(false);

    useEffect(() => {
        let current = true;
        getEncryptionStatement().then(
            (result) => {
                if (current) setStatement(result);
            },
            () => {
                if (current) setFailed(true);
            },
        );
        return () => { current = false; };
    }, []);

    return (
        <section className="encryption-status" aria-label="Privacy proof">
            <h2>Privacy proof</h2>
            {statement ? (
                <>
                    <p className="encryption-status-filevault" data-state={statement.filevault}>FileVault is {statement.filevault}</p>
                    <div className="encryption-status-columns">
                        <div>
                            <h3>Encrypted by FNDR</h3>
                            <ul className="is-sealed">{statement.encrypted.map((line) => <li key={line}>{line}</li>)}</ul>
                        </div>
                        <div>
                            <h3>Other local data</h3>
                            <ul className="is-open">{statement.not_encrypted.map((line) => <li key={line}>{line}</li>)}</ul>
                        </div>
                    </div>
                </>
            ) : (
                <p role="status">{failed ? "FileVault status could not be checked." : "Checking FileVault status…"}</p>
            )}
        </section>
    );
}
