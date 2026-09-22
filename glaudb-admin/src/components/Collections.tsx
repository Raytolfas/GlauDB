import { useEffect, useState } from "react";
import { Plus, Trash, Database, Eye, Upload, X } from "lucide-react";
import { authorizedFetch } from "../api";

interface FieldSchema {
  name: string;
  field_type: any;
  required: boolean;
  unique: boolean;
}

interface CollectionSchema {
  name: string;
  fields: FieldSchema[];
}

export default function Collections({ token }: { token: string }) {
  const [collections, setCollections] = useState<CollectionSchema[]>([]);
  const [selectedCol, setSelectedCol] = useState<CollectionSchema | null>(null);
  
  // Tabs: 'records' | 'schema'
  const [activeSubTab, setActiveSubTab] = useState<'records' | 'schema'>('records');

  // List records state
  const [records, setRecords] = useState<any[]>([]);
  const [loadingRecords, setLoadingRecords] = useState(false);

  // Edit/New Record modal state
  const [editingRecord, setEditingRecord] = useState<any | null>(null); // null = not editing. {} = new record.
  const [isNewRecord, setIsNewRecord] = useState(false);

  // Collection creator modal
  const [showColCreator, setShowColCreator] = useState(false);
  const [newColName, setNewColName] = useState("");

  // Load collections
  const loadCollections = async () => {
    try {
      const res = await authorizedFetch(token, "/api/collections");
      if (res.ok) {
        const data = await res.json();
        setCollections(data);
        if (data.length > 0 && !selectedCol) {
          setSelectedCol(data[0]);
        }
      }
    } catch (err) {
      console.error("Error loading collections:", err);
    }
  };

  useEffect(() => {
    loadCollections();
  }, []);

  // Load records when selected collection changes
  const loadRecords = async (colName: string) => {
    setLoadingRecords(true);
    try {
      const res = await authorizedFetch(token, `/api/collections/${colName}/records`);
      if (res.ok) {
        const data = await res.json();
        setRecords(data);
      }
    } catch (err) {
      console.error("Error loading records:", err);
    } finally {
      setLoadingRecords(false);
    }
  };

  useEffect(() => {
    if (selectedCol) {
      loadRecords(selectedCol.name);
      setActiveSubTab('records');
    } else {
      setRecords([]);
    }
  }, [selectedCol]);

  // Create collection
  const handleCreateCollection = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newColName.trim()) return;

    const schema: CollectionSchema = {
      name: newColName.toLowerCase().replace(/[^a-z0-9_]/g, ""),
      fields: [
        { name: "title", field_type: "text", required: false, unique: false }
      ]
    };

    try {
      const res = await authorizedFetch(token, "/api/collections", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(schema),
      });

      if (res.ok) {
        setShowColCreator(false);
        setNewColName("");
        await loadCollections();
        const created = collections.find(c => c.name === schema.name) || schema;
        setSelectedCol(created);
      } else {
        const err = await res.json();
        alert(err.error || "Failed to create collection");
      }
    } catch (err) {
      alert("Error connecting to server");
    }
  };

  // Delete collection
  const handleDeleteCollection = async (name: string) => {
    if (!confirm(`Are you sure you want to delete collection "${name}"?`)) return;

    try {
      const res = await authorizedFetch(token, `/api/collections/${name}`, {
        method: "DELETE",
      });

      if (res.ok) {
        setSelectedCol(null);
        await loadCollections();
      }
    } catch (err) {
      alert("Error deleting collection");
    }
  };

  // Add/Update schema field
  const handleSaveSchema = async (updatedFields: FieldSchema[]) => {
    if (!selectedCol) return;

    const updatedSchema = {
      ...selectedCol,
      fields: updatedFields
    };

    try {
      const res = await authorizedFetch(token, "/api/collections", {
        method: "POST", // Overwrites if already exists in core
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(updatedSchema),
      });

      if (res.ok) {
        setSelectedCol(updatedSchema);
        await loadCollections();
        alert("Schema updated.");
      } else {
        const err = await res.json();
        alert(err.error || "Failed to update schema");
      }
    } catch (err) {
      alert("Error updating schema");
    }
  };

  // Record CRUD helpers
  const handleSaveRecord = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedCol || !editingRecord) return;

    const url = isNewRecord
      ? `/api/collections/${selectedCol.name}/records`
      : `/api/collections/${selectedCol.name}/records/${editingRecord.id}`;

    const method = isNewRecord ? "POST" : "PUT";

    // Strip internal read-only fields for submission
    const submitData = { ...editingRecord };
    delete submitData.id;
    delete submitData.created;
    delete submitData.updated;

    try {
      const res = await authorizedFetch(token, url, {
        method,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(submitData),
      });

      if (res.ok) {
        setEditingRecord(null);
        loadRecords(selectedCol.name);
      } else {
        const err = await res.json();
        alert(err.error || "Failed to save record");
      }
    } catch (err) {
      alert("Error saving record");
    }
  };

  const handleDeleteRecord = async (id: string) => {
    if (!selectedCol || !confirm("Are you sure?")) return;

    try {
      const res = await authorizedFetch(token, `/api/collections/${selectedCol.name}/records/${id}`, {
        method: "DELETE",
      });

      if (res.ok) {
        setEditingRecord(null);
        loadRecords(selectedCol.name);
      }
    } catch (err) {
      alert("Error deleting record");
    }
  };

  const handleFileUpload = async (fieldName: string, file: File) => {
    if (!selectedCol) return;

    const formData = new FormData();
    formData.append("file", file);

    try {
      const res = await authorizedFetch(token, "/api/upload", {
        method: "POST",
        body: formData,
      });

      if (res.ok) {
        const data = await res.json(); // returns { url, name, size }
        setEditingRecord((prev: any) => ({
          ...prev,
          [fieldName]: data // store S3 URL info object
        }));
      } else {
        alert("File upload failed.");
      }
    } catch (e) {
      alert("Error uploading file.");
    }
  };

  const getFileDisplay = (val: any) => {
    if (!val) return "-";
    if (typeof val === "string") return <a href={val} target="_blank" rel="noreferrer" className="underline font-mono text-xs hover:text-black">link</a>;
    if (typeof val === "object" && val.url) {
      return (
        <a href={val.url} target="_blank" rel="noreferrer" className="flex items-center space-x-1 underline text-xs font-mono hover:text-black">
          <Eye className="h-3 w-3" />
          <span>{val.name || "file"}</span>
        </a>
      );
    }
    return "-";
  };

  return (
    <div className="flex h-[calc(100vh-100px)] border border-neutral-200 bg-white rounded-lg shadow-sm overflow-hidden">
      {/* 1. Left sidebar collections list */}
      <div className="w-64 border-r border-neutral-200 bg-neutral-50 flex flex-col justify-between rounded-l-lg">
        <div>
          <div className="flex items-center justify-between border-b border-neutral-200 px-4 py-3">
            <span className="text-xs font-bold uppercase tracking-wider text-neutral-500">Collections</span>
            <button
              onClick={() => setShowColCreator(true)}
              className="border border-neutral-300 hover:border-neutral-900 p-1 text-neutral-700 bg-white rounded-md"
            >
              <Plus className="h-3 w-3" />
            </button>
          </div>
          <div className="divide-y divide-neutral-200 max-h-[calc(100vh-200px)] overflow-y-auto">
            {collections.map((col) => (
              <button
                key={col.name}
                onClick={() => setSelectedCol(col)}
                className={`w-full flex items-center justify-between px-4 py-2.5 text-left text-xs font-mono tracking-tight transition-colors ${
                  selectedCol?.name === col.name
                    ? "bg-white text-black font-bold border-l-2 border-black"
                    : "text-neutral-600 hover:bg-neutral-100"
                }`}
              >
                <div className="flex items-center space-x-2">
                  <Database className="h-3.5 w-3.5" />
                  <span>{col.name}</span>
                </div>
              </button>
            ))}
          </div>
        </div>
      </div>

      {/* 2. Main Work Panel */}
      <div className="flex-1 flex flex-col">
        {selectedCol ? (
          <>
            {/* Header bar */}
            <div className="flex items-center justify-between border-b border-neutral-200 px-6 py-3.5 bg-neutral-50">
              <div className="flex items-center space-x-4">
                <h2 className="text-sm font-bold font-mono tracking-tight text-neutral-900">{selectedCol.name}</h2>
                <div className="flex border border-neutral-300 bg-white rounded-md overflow-hidden">
                  <button
                    onClick={() => setActiveSubTab('records')}
                    className={`px-3 py-1 text-xs font-bold uppercase ${
                      activeSubTab === 'records' ? "bg-neutral-950 text-white" : "hover:bg-neutral-100 text-neutral-600"
                    }`}
                  >
                    Records
                  </button>
                  <button
                    onClick={() => setActiveSubTab('schema')}
                    className={`px-3 py-1 text-xs font-bold uppercase border-l border-neutral-300 ${
                      activeSubTab === 'schema' ? "bg-neutral-950 text-white" : "hover:bg-neutral-100 text-neutral-600"
                    }`}
                  >
                    Schema
                  </button>
                </div>
              </div>

              <div className="flex items-center space-x-2">
                {activeSubTab === 'records' && (
                  <button
                    onClick={() => {
                      setEditingRecord({});
                      setIsNewRecord(true);
                    }}
                    className="border border-neutral-950 bg-neutral-950 px-3 py-1 text-xs font-bold uppercase tracking-wider text-white hover:bg-neutral-800 rounded-md"
                  >
                    New Record
                  </button>
                )}
                <button
                  onClick={() => handleDeleteCollection(selectedCol.name)}
                  className="border border-red-300 hover:border-red-600 hover:bg-red-50 p-1.5 text-red-600 bg-white rounded-md"
                >
                  <Trash className="h-3.5 w-3.5" />
                </button>
              </div>
            </div>

            {/* Content view */}
            <div className="flex-1 overflow-auto p-6">
              {activeSubTab === 'records' ? (
                /* RECORDS EXPLORER */
                loadingRecords ? (
                  <div className="text-xs font-mono text-neutral-500">Loading records...</div>
                ) : records.length === 0 ? (
                  <div className="border border-dashed border-neutral-300 p-8 text-center text-xs font-mono text-neutral-400">
                    No records found. Click "New Record" to add data.
                  </div>
                ) : (
                  <div className="border border-neutral-200 overflow-x-auto">
                    <table className="w-full text-left border-collapse text-xs font-mono">
                      <thead>
                        <tr className="bg-neutral-50 border-b border-neutral-200 uppercase text-[10px] tracking-wider text-neutral-500 font-bold">
                          <th className="px-4 py-2 border-r border-neutral-200">id</th>
                          {selectedCol.fields.map((f) => (
                            <th key={f.name} className="px-4 py-2 border-r border-neutral-200">{f.name}</th>
                          ))}
                          <th className="px-4 py-2 border-r border-neutral-200">created</th>
                          <th className="px-4 py-2">updated</th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-neutral-200 bg-white">
                        {records.map((rec) => (
                          <tr
                            key={rec.id}
                            onClick={() => {
                              setEditingRecord(rec);
                              setIsNewRecord(false);
                            }}
                            className="hover:bg-neutral-50 cursor-pointer"
                          >
                            <td className="px-4 py-2.5 font-bold border-r border-neutral-200 max-w-[120px] truncate">{rec.id}</td>
                            {selectedCol.fields.map((f) => {
                              const val = rec[f.name];
                              return (
                                <td key={f.name} className="px-4 py-2.5 border-r border-neutral-200 max-w-[150px] truncate">
                                  {f.field_type === "file" ? (
                                    getFileDisplay(val)
                                  ) : typeof val === "object" ? (
                                    JSON.stringify(val)
                                  ) : (
                                    String(val ?? "-")
                                  )}
                                </td>
                              );
                            })}
                            <td className="px-4 py-2.5 border-r border-neutral-200 max-w-[120px] truncate text-neutral-400">
                              {rec.created?.slice(0, 19).replace('T', ' ')}
                            </td>
                            <td className="px-4 py-2.5 max-w-[120px] truncate text-neutral-400">
                              {rec.updated?.slice(0, 19).replace('T', ' ')}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )
              ) : (
                /* SCHEMA DESIGNER */
                <SchemaDesigner
                  schema={selectedCol}
                  onSave={handleSaveSchema}
                />
              )}
            </div>
          </>
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center text-xs font-mono text-neutral-400">
            <Database className="h-8 w-8 mb-2 text-neutral-300" />
            <span>Select or create a collection from the sidebar.</span>
          </div>
        )}
      </div>

      {/* 3. New / Edit Record side drawer */}
      {editingRecord && (
        <div className="fixed inset-y-0 right-0 w-[420px] border-l border-neutral-200 bg-white shadow-lg p-6 z-50 overflow-y-auto flex flex-col justify-between">
          <div>
            <div className="flex items-center justify-between border-b border-neutral-200 pb-4 mb-4">
              <h3 className="text-sm font-bold uppercase tracking-wider text-neutral-900">
                {isNewRecord ? "New Record" : "Edit Record"}
              </h3>
              <button onClick={() => setEditingRecord(null)} className="text-neutral-400 hover:text-neutral-900">
                <X className="h-4 w-4" />
              </button>
            </div>

            <form onSubmit={handleSaveRecord} className="space-y-4">
              {!isNewRecord && (
                <div>
                  <label className="block text-[10px] font-bold uppercase tracking-wider text-neutral-400 mb-1">id</label>
                  <input
                    type="text"
                    value={editingRecord.id || ""}
                    disabled
                    className="w-full bg-neutral-50 border border-neutral-200 px-3 py-1.5 text-xs font-mono text-neutral-400 rounded-none cursor-not-allowed"
                  />
                </div>
              )}

              {selectedCol?.fields.map((field) => {
                const val = editingRecord[field.name];
                return (
                  <div key={field.name}>
                    <div className="flex justify-between items-center mb-1">
                      <label className="block text-[10px] font-bold uppercase tracking-wider text-neutral-500 font-mono">
                        {field.name}
                        {field.required && <span className="text-red-500 ml-0.5">*</span>}
                      </label>
                      <span className="text-[9px] text-neutral-400 font-mono">({typeof field.field_type === 'string' ? field.field_type : 'relation'})</span>
                    </div>

                    {field.field_type === "bool" ? (
                      <select
                        value={val === true ? "true" : "false"}
                        onChange={(e) => setEditingRecord({ ...editingRecord, [field.name]: e.target.value === "true" })}
                        className="w-full border border-neutral-300 px-3 py-1.5 text-xs font-mono focus:border-neutral-950 focus:outline-none bg-white rounded-none"
                      >
                        <option value="false">false</option>
                        <option value="true">true</option>
                      </select>
                    ) : field.field_type === "json" ? (
                      <textarea
                        value={typeof val === 'object' ? JSON.stringify(val, null, 2) : val || ""}
                        onChange={(e) => {
                          try {
                            const parsed = JSON.parse(e.target.value);
                            setEditingRecord({ ...editingRecord, [field.name]: parsed });
                          } catch (_) {
                            setEditingRecord({ ...editingRecord, [field.name]: e.target.value });
                          }
                        }}
                        rows={3}
                        className="w-full border border-neutral-300 px-3 py-1.5 text-xs font-mono focus:border-neutral-950 focus:outline-none rounded-none"
                        placeholder="{}"
                      />
                    ) : field.field_type === "file" ? (
                      <div className="space-y-2 border border-neutral-200 p-3 bg-neutral-50">
                        {val ? (
                          <div className="flex items-center justify-between">
                            {getFileDisplay(val)}
                            <button
                              type="button"
                              onClick={() => setEditingRecord({ ...editingRecord, [field.name]: null })}
                              className="text-red-500 hover:text-red-700"
                            >
                              <X className="h-3.5 w-3.5" />
                            </button>
                          </div>
                        ) : (
                          <label className="flex flex-col items-center justify-center border border-dashed border-neutral-300 bg-white p-4 cursor-pointer hover:bg-neutral-100">
                            <Upload className="h-4 w-4 text-neutral-400 mb-1" />
                            <span className="text-[10px] uppercase font-bold text-neutral-500">Upload File</span>
                            <input
                              type="file"
                              className="hidden"
                              onChange={(e) => {
                                const file = e.target.files?.[0];
                                if (file) handleFileUpload(field.name, file);
                              }}
                            />
                          </label>
                        )}
                      </div>
                    ) : (
                      <input
                        type={field.field_type === "number" ? "number" : "text"}
                        step="any"
                        value={val ?? ""}
                        onChange={(e) =>
                          setEditingRecord({
                            ...editingRecord,
                            [field.name]: field.field_type === "number" ? parseFloat(e.target.value) || 0 : e.target.value
                          })
                        }
                        className="w-full border border-neutral-300 px-3 py-1.5 text-xs font-mono focus:border-neutral-950 focus:outline-none rounded-none"
                        required={field.required}
                      />
                    )}
                  </div>
                );
              })}
            </form>
          </div>

          <div className="mt-8 flex justify-between border-t border-neutral-200 pt-4 bg-white">
            {!isNewRecord ? (
              <button
                type="button"
                onClick={() => handleDeleteRecord(editingRecord.id)}
                className="border border-red-500 bg-white text-red-500 hover:bg-red-50 px-4 py-2 text-xs font-bold uppercase tracking-wider rounded-none"
              >
                Delete Record
              </button>
            ) : <div />}
            <div className="flex space-x-2">
              <button
                type="button"
                onClick={() => setEditingRecord(null)}
                className="border border-neutral-300 hover:border-neutral-900 bg-white px-4 py-2 text-xs font-bold uppercase tracking-wider text-neutral-700 rounded-none"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleSaveRecord}
                className="border border-neutral-950 bg-neutral-950 text-white hover:bg-neutral-800 px-4 py-2 text-xs font-bold uppercase tracking-wider rounded-none"
              >
                Save
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 4. Collection Creator Modal */}
      {showColCreator && (
        <div className="fixed inset-0 bg-black/30 z-50 flex items-center justify-center p-4">
          <div className="bg-white border border-neutral-200 max-w-sm w-full p-6 shadow-xl">
            <h3 className="text-sm font-bold uppercase tracking-wider text-neutral-950 mb-4">Create Collection</h3>
            <form onSubmit={handleCreateCollection} className="space-y-4">
              <div>
                <label className="block text-xs font-bold uppercase text-neutral-500 mb-1">Collection Name</label>
                <input
                  type="text"
                  value={newColName}
                  onChange={(e) => setNewColName(e.target.value)}
                  placeholder="e.g. posts"
                  className="w-full border border-neutral-300 px-3 py-1.5 text-xs font-mono focus:border-neutral-950 focus:outline-none rounded-none"
                  required
                />
              </div>
              <div className="flex justify-end space-x-2 pt-2">
                <button
                  type="button"
                  onClick={() => setShowColCreator(false)}
                  className="border border-neutral-300 hover:border-neutral-900 px-4 py-2 text-xs font-bold uppercase tracking-wider text-neutral-700 rounded-none"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="border border-neutral-950 bg-neutral-950 text-white hover:bg-neutral-800 px-4 py-2 text-xs font-bold uppercase tracking-wider rounded-none"
                >
                  Create
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}

/* SCHEMA DESIGNER SUB-COMPONENT */
interface SchemaDesignerProps {
  schema: CollectionSchema;
  onSave: (fields: FieldSchema[]) => void;
}

function SchemaDesigner({ schema, onSave }: SchemaDesignerProps) {
  const [fields, setFields] = useState<FieldSchema[]>([]);

  useEffect(() => {
    setFields([...schema.fields]);
  }, [schema]);

  const handleAddField = () => {
    setFields([
      ...fields,
      { name: `field_${fields.length + 1}`, field_type: "text", required: false, unique: false }
    ]);
  };

  const handleRemoveField = (index: number) => {
    const next = [...fields];
    next.splice(index, 1);
    setFields(next);
  };

  const handleFieldChange = (index: number, key: keyof FieldSchema, value: any) => {
    const next = [...fields];
    next[index] = {
      ...next[index],
      [key]: value
    };
    setFields(next);
  };

  return (
    <div className="max-w-3xl space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-bold uppercase tracking-wider text-neutral-900 font-mono">Collection Fields</h3>
        <button
          onClick={handleAddField}
          className="border border-neutral-300 hover:border-neutral-900 px-3 py-1.5 text-xs font-bold uppercase tracking-wider bg-white rounded-md"
        >
          Add Field
        </button>
      </div>

      <div className="border border-neutral-200 bg-white divide-y divide-neutral-200 rounded-lg overflow-hidden">
        {fields.length === 0 ? (
          <div className="p-4 text-xs font-mono text-neutral-400 text-center">
            No fields defined. Add at least one field.
          </div>
        ) : (
          fields.map((field, idx) => (
            <div key={idx} className="p-4 flex items-center space-x-4">
              <input
                type="text"
                value={field.name}
                onChange={(e) => handleFieldChange(idx, "name", e.target.value.toLowerCase().replace(/[^a-z0-9_]/g, ""))}
                placeholder="field_name"
                className="flex-1 border border-neutral-300 px-3 py-1.5 text-xs font-mono focus:border-neutral-950 focus:outline-none rounded-md"
              />

              <select
                value={typeof field.field_type === 'string' ? field.field_type : 'relation'}
                onChange={(e) => {
                  const val = e.target.value;
                  if (val === 'relation') {
                    handleFieldChange(idx, "field_type", { collection: "users" });
                  } else {
                    handleFieldChange(idx, "field_type", val);
                  }
                }}
                className="border border-neutral-300 px-3 py-1.5 text-xs font-mono focus:border-neutral-950 focus:outline-none bg-white rounded-md"
              >
                <option value="text">text</option>
                <option value="number">number</option>
                <option value="bool">bool</option>
                <option value="json">json</option>
                <option value="datetime">datetime</option>
                <option value="file">file</option>
              </select>

              <div className="flex items-center space-x-2">
                <input
                  type="checkbox"
                  id={`req-${idx}`}
                  checked={field.required}
                  onChange={(e) => handleFieldChange(idx, "required", e.target.checked)}
                  className="h-4 w-4 border-neutral-300 text-neutral-950 focus:ring-neutral-950 rounded-none"
                />
                <label htmlFor={`req-${idx}`} className="text-xs font-bold text-neutral-500 uppercase select-none">
                  Required
                </label>
              </div>

              <div className="flex items-center space-x-2">
                <input
                  type="checkbox"
                  id={`uniq-${idx}`}
                  checked={field.unique}
                  onChange={(e) => handleFieldChange(idx, "unique", e.target.checked)}
                  className="h-4 w-4 border-neutral-300 text-neutral-950 focus:ring-neutral-950 rounded-none"
                />
                <label htmlFor={`uniq-${idx}`} className="text-xs font-bold text-neutral-500 uppercase select-none">
                  Unique
                </label>
              </div>

              <button
                onClick={() => handleRemoveField(idx)}
                className="text-red-500 hover:text-red-700 p-1"
              >
                <Trash className="h-4 w-4" />
              </button>
            </div>
          ))
        )}
      </div>

      <div className="pt-2">
        <button
          onClick={() => onSave(fields)}
          className="border border-neutral-950 bg-neutral-950 px-4 py-2 text-xs font-bold uppercase tracking-wider text-white hover:bg-neutral-800 rounded-md"
        >
          Save Schema
        </button>
      </div>
    </div>
  );
}
