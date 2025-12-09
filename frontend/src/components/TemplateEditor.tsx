'use client';

import React, { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { 
  Select, 
  SelectContent, 
  SelectItem, 
  SelectTrigger, 
  SelectValue 
} from '@/components/ui/select';
import { 
  Plus, 
  Trash2, 
  Save, 
  FolderOpen, 
  FileText, 
  Edit3, 
  Copy,
  ChevronDown,
  ChevronUp,
  GripVertical
} from 'lucide-react';
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '@/components/ui/accordion';

interface TemplateSection {
  title: string;
  instruction: string;
  format: 'paragraph' | 'list' | 'string';
  item_format?: string;
  example_item_format?: string;
}

interface Template {
  name: string;
  description: string;
  sections: TemplateSection[];
}

interface TemplateInfo {
  id: string;
  name: string;
  description: string;
  isCustom?: boolean;
}

export function TemplateEditor() {
  const [templates, setTemplates] = useState<TemplateInfo[]>([]);
  const [selectedTemplateId, setSelectedTemplateId] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isEditing, setIsEditing] = useState(false);
  const [templatesDir, setTemplatesDir] = useState<string>('');
  
  // Current template being edited
  const [templateId, setTemplateId] = useState('');
  const [templateName, setTemplateName] = useState('');
  const [templateDescription, setTemplateDescription] = useState('');
  const [sections, setSections] = useState<TemplateSection[]>([]);

  // Load templates list
  const loadTemplates = useCallback(async () => {
    try {
      const templateList = await invoke('api_list_templates') as TemplateInfo[];
      
      // Check which templates are custom
      const templatesWithCustomFlag = await Promise.all(
        templateList.map(async (t) => {
          try {
            const isCustom = await invoke('api_is_custom_template', { templateId: t.id }) as boolean;
            return { ...t, isCustom };
          } catch {
            return { ...t, isCustom: false };
          }
        })
      );
      
      setTemplates(templatesWithCustomFlag);
    } catch (error) {
      console.error('Failed to load templates:', error);
      toast.error('Failed to load templates');
    }
  }, []);

  // Load templates directory path
  useEffect(() => {
    const loadDir = async () => {
      try {
        const dir = await invoke('api_get_templates_directory') as string;
        setTemplatesDir(dir);
      } catch (error) {
        console.error('Failed to get templates directory:', error);
      }
    };
    loadDir();
    loadTemplates();
  }, [loadTemplates]);

  // Load a template for editing
  const loadTemplate = async (id: string) => {
    setIsLoading(true);
    try {
      const json = await invoke('api_get_template_json', { templateId: id }) as string;
      const template = JSON.parse(json) as Template;
      
      setSelectedTemplateId(id);
      setTemplateId(id);
      setTemplateName(template.name);
      setTemplateDescription(template.description);
      setSections(template.sections);
      setIsEditing(true);
    } catch (error) {
      console.error('Failed to load template:', error);
      toast.error('Failed to load template');
    }
    setIsLoading(false);
  };

  // Create new template
  const createNewTemplate = () => {
    setSelectedTemplateId(null);
    setTemplateId('');
    setTemplateName('New Template');
    setTemplateDescription('Description of your template');
    setSections([
      {
        title: 'Summary',
        instruction: 'Provide a brief summary of the meeting',
        format: 'paragraph'
      }
    ]);
    setIsEditing(true);
  };

  // Duplicate template
  const duplicateTemplate = async (id: string) => {
    await loadTemplate(id);
    setSelectedTemplateId(null);
    setTemplateId(`${id}_copy`);
    setTemplateName(`${templateName} (Copy)`);
  };

  // Save template
  const saveTemplate = async () => {
    if (!templateId.trim()) {
      toast.error('Template ID is required');
      return;
    }
    if (!templateName.trim()) {
      toast.error('Template name is required');
      return;
    }
    if (sections.length === 0) {
      toast.error('At least one section is required');
      return;
    }

    const template: Template = {
      name: templateName,
      description: templateDescription,
      sections: sections.map(s => ({
        title: s.title,
        instruction: s.instruction,
        format: s.format,
        ...(s.item_format && { item_format: s.item_format }),
        ...(s.example_item_format && { example_item_format: s.example_item_format })
      }))
    };

    try {
      const templateJson = JSON.stringify(template, null, 2);
      await invoke('api_save_custom_template', { 
        templateId: templateId.toLowerCase().replace(/\s+/g, '_'),
        templateJson 
      });
      toast.success(`Template "${templateName}" saved successfully`);
      await loadTemplates();
      setIsEditing(false);
    } catch (error) {
      console.error('Failed to save template:', error);
      toast.error(`Failed to save template: ${error}`);
    }
  };

  // Delete template
  const deleteTemplate = async (id: string) => {
    try {
      await invoke('api_delete_custom_template', { templateId: id });
      toast.success('Template deleted');
      await loadTemplates();
      if (selectedTemplateId === id) {
        setIsEditing(false);
        setSelectedTemplateId(null);
      }
    } catch (error) {
      console.error('Failed to delete template:', error);
      toast.error(`${error}`);
    }
  };

  // Section management
  const addSection = () => {
    setSections([...sections, {
      title: 'New Section',
      instruction: 'Instructions for this section',
      format: 'paragraph'
    }]);
  };

  const updateSection = (index: number, field: keyof TemplateSection, value: string) => {
    const newSections = [...sections];
    newSections[index] = { ...newSections[index], [field]: value };
    setSections(newSections);
  };

  const removeSection = (index: number) => {
    setSections(sections.filter((_, i) => i !== index));
  };

  const moveSection = (index: number, direction: 'up' | 'down') => {
    const newIndex = direction === 'up' ? index - 1 : index + 1;
    if (newIndex < 0 || newIndex >= sections.length) return;
    
    const newSections = [...sections];
    [newSections[index], newSections[newIndex]] = [newSections[newIndex], newSections[index]];
    setSections(newSections);
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-lg font-semibold">Meeting Templates</h3>
          <p className="text-sm text-gray-500">
            Create and edit custom templates for meeting summaries
          </p>
        </div>
        <Button onClick={createNewTemplate} className="gap-2">
          <Plus className="w-4 h-4" />
          New Template
        </Button>
      </div>

      {/* Templates List */}
      {!isEditing && (
        <div className="space-y-3">
          {templates.map((template) => (
            <div
              key={template.id}
              className="flex items-center justify-between p-4 border rounded-lg hover:bg-gray-50 transition-colors"
            >
              <div className="flex items-center gap-3">
                <FileText className="w-5 h-5 text-gray-400" />
                <div>
                  <div className="flex items-center gap-2">
                    <span className="font-medium">{template.name}</span>
                    {template.isCustom && (
                      <span className="text-xs bg-blue-100 text-blue-700 px-2 py-0.5 rounded">
                        Custom
                      </span>
                    )}
                  </div>
                  <p className="text-sm text-gray-500">{template.description}</p>
                </div>
              </div>
              <div className="flex items-center gap-2">
                <Button 
                  variant="ghost" 
                  size="sm"
                  onClick={() => duplicateTemplate(template.id)}
                  title="Duplicate"
                >
                  <Copy className="w-4 h-4" />
                </Button>
                <Button 
                  variant="ghost" 
                  size="sm"
                  onClick={() => loadTemplate(template.id)}
                  title="Edit"
                >
                  <Edit3 className="w-4 h-4" />
                </Button>
                {template.isCustom && (
                  <Button 
                    variant="ghost" 
                    size="sm"
                    onClick={() => deleteTemplate(template.id)}
                    className="text-red-600 hover:text-red-700 hover:bg-red-50"
                    title="Delete"
                  >
                    <Trash2 className="w-4 h-4" />
                  </Button>
                )}
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Template Editor */}
      {isEditing && (
        <div className="space-y-6 border rounded-lg p-6 bg-gray-50">
          <div className="flex items-center justify-between">
            <h4 className="font-semibold">
              {selectedTemplateId ? 'Edit Template' : 'New Template'}
            </h4>
            <div className="flex gap-2">
              <Button variant="outline" onClick={() => setIsEditing(false)}>
                Cancel
              </Button>
              <Button onClick={saveTemplate} className="gap-2">
                <Save className="w-4 h-4" />
                Save Template
              </Button>
            </div>
          </div>

          {/* Basic Info */}
          <div className="grid grid-cols-2 gap-4">
            <div>
              <Label>Template ID</Label>
              <Input
                value={templateId}
                onChange={(e) => setTemplateId(e.target.value.toLowerCase().replace(/\s+/g, '_'))}
                placeholder="my_template"
                className="mt-1"
              />
              <p className="text-xs text-gray-500 mt-1">
                Unique identifier (lowercase, underscores)
              </p>
            </div>
            <div>
              <Label>Template Name</Label>
              <Input
                value={templateName}
                onChange={(e) => setTemplateName(e.target.value)}
                placeholder="My Custom Template"
                className="mt-1"
              />
            </div>
          </div>

          <div>
            <Label>Description</Label>
            <Input
              value={templateDescription}
              onChange={(e) => setTemplateDescription(e.target.value)}
              placeholder="What this template is used for"
              className="mt-1"
            />
          </div>

          {/* Sections */}
          <div>
            <div className="flex items-center justify-between mb-3">
              <Label>Sections</Label>
              <Button variant="outline" size="sm" onClick={addSection} className="gap-1">
                <Plus className="w-3 h-3" />
                Add Section
              </Button>
            </div>

            <div className="space-y-4">
              {sections.map((section, index) => (
                <div key={index} className="border rounded-lg p-4 bg-white">
                  <div className="flex items-start justify-between mb-3">
                    <div className="flex items-center gap-2">
                      <GripVertical className="w-4 h-4 text-gray-400" />
                      <span className="font-medium text-sm">Section {index + 1}</span>
                    </div>
                    <div className="flex items-center gap-1">
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => moveSection(index, 'up')}
                        disabled={index === 0}
                      >
                        <ChevronUp className="w-4 h-4" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => moveSection(index, 'down')}
                        disabled={index === sections.length - 1}
                      >
                        <ChevronDown className="w-4 h-4" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => removeSection(index)}
                        className="text-red-600 hover:text-red-700"
                        disabled={sections.length === 1}
                      >
                        <Trash2 className="w-4 h-4" />
                      </Button>
                    </div>
                  </div>

                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <Label className="text-xs">Title</Label>
                      <Input
                        value={section.title}
                        onChange={(e) => updateSection(index, 'title', e.target.value)}
                        placeholder="Section Title"
                        className="mt-1"
                      />
                    </div>
                    <div>
                      <Label className="text-xs">Format</Label>
                      <Select
                        value={section.format}
                        onValueChange={(value) => updateSection(index, 'format', value)}
                      >
                        <SelectTrigger className="mt-1">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="paragraph">Paragraph</SelectItem>
                          <SelectItem value="list">List</SelectItem>
                          <SelectItem value="string">String</SelectItem>
                        </SelectContent>
                      </Select>
                    </div>
                  </div>

                  <div className="mt-3">
                    <Label className="text-xs">Instruction for AI</Label>
                    <Textarea
                      value={section.instruction}
                      onChange={(e) => updateSection(index, 'instruction', e.target.value)}
                      placeholder="Instructions for what the AI should extract for this section"
                      className="mt-1"
                      rows={2}
                    />
                  </div>

                  {section.format === 'list' && (
                    <Accordion type="single" collapsible className="mt-3">
                      <AccordionItem value="advanced">
                        <AccordionTrigger className="text-xs text-gray-500">
                          Advanced: Item Format (optional)
                        </AccordionTrigger>
                        <AccordionContent>
                          <Textarea
                            value={section.item_format || section.example_item_format || ''}
                            onChange={(e) => updateSection(index, 'item_format', e.target.value)}
                            placeholder="| **Column1** | **Column2** |\n| --- | --- |"
                            className="mt-1 font-mono text-sm"
                            rows={3}
                          />
                          <p className="text-xs text-gray-500 mt-1">
                            Markdown format for list items (e.g., table structure)
                          </p>
                        </AccordionContent>
                      </AccordionItem>
                    </Accordion>
                  )}
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      {/* Templates Directory Info */}
      <div className="text-xs text-gray-500 flex items-center gap-2 border-t pt-4">
        <FolderOpen className="w-4 h-4" />
        <span>Custom templates are stored in: <code className="bg-gray-100 px-1 py-0.5 rounded">{templatesDir}</code></span>
      </div>
    </div>
  );
}
